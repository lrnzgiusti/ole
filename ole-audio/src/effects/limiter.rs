//! Professional True Peak Limiter
//!
//! State-of-the-art brickwall limiter with:
//! - 4x oversampled true peak detection (ITU-R BS.1770 compliant)
//! - O(1) constant-time peak-hold using monotonic deque (Signalsmith algorithm)
//! - Two-stage envelope (fast transient + slow release)
//! - Soft knee gain calculation (1.5 dB knee)
//! - Anticipatory gain curve applied before peaks arrive
//! - 5ms lookahead for guaranteed ceiling compliance

use super::Effect;
use std::collections::VecDeque;

/// Lookahead time in milliseconds
const LOOKAHEAD_MS: f32 = 5.0;

/// Soft knee width in dB
const KNEE_DB: f32 = 1.5;

/// Default ceiling in dBFS
const DEFAULT_CEILING_DB: f32 = -1.0;

/// Fast envelope attack time in ms (catches transients)
const FAST_ATTACK_MS: f32 = 0.1;
/// Fast envelope release time in ms (punchy recovery)
const FAST_RELEASE_MS: f32 = 10.0;

/// Slow envelope attack time in ms (sustained content)
const SLOW_ATTACK_MS: f32 = 2.0;
/// Slow envelope release time in ms (no pumping)
const SLOW_RELEASE_MS: f32 = 80.0;

/// 4x oversampling polyphase FIR filter coefficients
/// Half-band filter designed for true peak detection
/// 16 taps per phase, 4 phases = 64 total coefficients
const OVERSAMPLING_TAPS: usize = 16;
const OVERSAMPLING_FACTOR: usize = 4;

/// Pre-computed polyphase FIR coefficients for 4x oversampling
/// Designed with Kaiser window, beta=8, for good stopband attenuation
const POLYPHASE_COEFFS: [[f32; OVERSAMPLING_TAPS]; OVERSAMPLING_FACTOR] = [
    // Phase 0 (original samples pass through)
    [
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    ],
    // Phase 1 (1/4 sample offset)
    [
        -0.00110, 0.00398, -0.01025, 0.02106, -0.03830, 0.06525, -0.11162, 0.59860, 0.59860,
        -0.11162, 0.06525, -0.03830, 0.02106, -0.01025, 0.00398, -0.00110,
    ],
    // Phase 2 (1/2 sample offset - peak location)
    [
        -0.00156, 0.00562, -0.01450, 0.02978, -0.05417, 0.09226, -0.15785, 0.67000, 0.67000,
        -0.15785, 0.09226, -0.05417, 0.02978, -0.01450, 0.00562, -0.00156,
    ],
    // Phase 3 (3/4 sample offset)
    [
        -0.00110, 0.00398, -0.01025, 0.02106, -0.03830, 0.06525, -0.11162, 0.59860, 0.59860,
        -0.11162, 0.06525, -0.03830, 0.02106, -0.01025, 0.00398, -0.00110,
    ],
];

/// True Peak Detector using 4x oversampling
///
/// Detects inter-sample peaks that can cause DAC clipping.
/// Uses polyphase FIR filtering for efficient oversampling.
struct TruePeakDetector {
    /// History buffer for FIR filtering (per channel)
    history_l: [f32; OVERSAMPLING_TAPS],
    history_r: [f32; OVERSAMPLING_TAPS],
    /// Current position in history buffer
    history_pos: usize,
}

impl TruePeakDetector {
    fn new() -> Self {
        Self {
            history_l: [0.0; OVERSAMPLING_TAPS],
            history_r: [0.0; OVERSAMPLING_TAPS],
            history_pos: 0,
        }
    }

    /// Detect the true peak of a stereo sample pair
    ///
    /// Returns the maximum absolute amplitude across all 4x oversampled points
    #[inline]
    fn detect(&mut self, left: f32, right: f32) -> f32 {
        // Add new samples to history
        self.history_l[self.history_pos] = left;
        self.history_r[self.history_pos] = right;

        let mut max_peak: f32 = 0.0;

        // Compute all 4 phases
        for phase in 0..OVERSAMPLING_FACTOR {
            let mut sum_l: f32 = 0.0;
            let mut sum_r: f32 = 0.0;

            // Convolve with polyphase coefficients
            for tap in 0..OVERSAMPLING_TAPS {
                let hist_idx = (self.history_pos + OVERSAMPLING_TAPS - tap) % OVERSAMPLING_TAPS;
                let coeff = POLYPHASE_COEFFS[phase][tap];
                sum_l += self.history_l[hist_idx] * coeff;
                sum_r += self.history_r[hist_idx] * coeff;
            }

            max_peak = max_peak.max(sum_l.abs()).max(sum_r.abs());
        }

        // Advance history position
        self.history_pos = (self.history_pos + 1) % OVERSAMPLING_TAPS;

        max_peak
    }

    fn reset(&mut self) {
        self.history_l.fill(0.0);
        self.history_r.fill(0.0);
        self.history_pos = 0;
    }
}

/// Entry in the peak-hold monotonic deque
#[derive(Clone, Copy)]
struct PeakEntry {
    /// Peak value
    value: f32,
    /// Expiration time (sample index when this entry expires)
    expires_at: u64,
}

/// O(1) Constant-Time Peak-Hold Buffer
///
/// Uses monotonic deque (Signalsmith algorithm) for sliding window maximum.
/// Amortized O(1) complexity per sample with no allocation in audio callback.
struct PeakHoldBuffer {
    /// Monotonic deque: values are decreasing from front to back
    /// Front always contains the current maximum
    deque: VecDeque<PeakEntry>,
    /// Hold time in samples
    hold_samples: usize,
    /// Current sample counter
    sample_counter: u64,
}

impl PeakHoldBuffer {
    fn new(hold_samples: usize) -> Self {
        Self {
            // Pre-allocate capacity to avoid runtime allocation
            deque: VecDeque::with_capacity(hold_samples + 1),
            hold_samples,
            sample_counter: 0,
        }
    }

    /// Push a new peak value and return the current maximum
    ///
    /// Maintains the invariant that deque is monotonically decreasing
    #[inline]
    fn push(&mut self, peak: f32) -> f32 {
        let expires_at = self.sample_counter + self.hold_samples as u64;

        // Remove expired entries from front
        while let Some(front) = self.deque.front() {
            if front.expires_at <= self.sample_counter {
                self.deque.pop_front();
            } else {
                break;
            }
        }

        // Remove entries smaller than new peak from back (maintain monotonic decreasing)
        while let Some(back) = self.deque.back() {
            if back.value <= peak {
                self.deque.pop_back();
            } else {
                break;
            }
        }

        // Push new entry
        self.deque.push_back(PeakEntry {
            value: peak,
            expires_at,
        });

        self.sample_counter += 1;

        // Front is always the maximum
        self.deque.front().map(|e| e.value).unwrap_or(0.0)
    }

    fn reset(&mut self) {
        self.deque.clear();
        self.sample_counter = 0;
    }
}

/// Two-Stage Envelope Follower
///
/// Combines fast transient catching with slow release to avoid pumping.
/// Uses minimum of both envelopes for comprehensive protection.
struct TwoStageEnvelope {
    /// Fast envelope (catches transients)
    fast_env: f32,
    /// Slow envelope (sustained content)
    slow_env: f32,
    /// Fast attack coefficient
    fast_attack: f32,
    /// Fast release coefficient
    fast_release: f32,
    /// Slow attack coefficient
    slow_attack: f32,
    /// Slow release coefficient
    slow_release: f32,
}

impl TwoStageEnvelope {
    fn new(sample_rate: f32) -> Self {
        Self {
            fast_env: 1.0,
            slow_env: 1.0,
            fast_attack: Self::time_to_coeff(FAST_ATTACK_MS, sample_rate),
            fast_release: Self::time_to_coeff(FAST_RELEASE_MS, sample_rate),
            slow_attack: Self::time_to_coeff(SLOW_ATTACK_MS, sample_rate),
            slow_release: Self::time_to_coeff(SLOW_RELEASE_MS, sample_rate),
        }
    }

    /// Convert time constant in ms to smoothing coefficient
    #[inline]
    fn time_to_coeff(time_ms: f32, sample_rate: f32) -> f32 {
        if time_ms <= 0.0 {
            return 0.0;
        }
        (-1.0 / (sample_rate * time_ms / 1000.0)).exp()
    }

    /// Process a target gain value through both envelope stages
    ///
    /// Returns the minimum of both envelopes (most aggressive limiting)
    #[inline]
    fn process(&mut self, target_gain: f32) -> f32 {
        // Fast envelope: instant attack, smooth release
        if target_gain < self.fast_env {
            // Attack - use coefficient for very fast but not instant response
            self.fast_env =
                self.fast_attack * self.fast_env + (1.0 - self.fast_attack) * target_gain;
            // Ensure we don't overshoot
            self.fast_env = self.fast_env.min(target_gain * 1.01);
        } else {
            // Release
            self.fast_env =
                self.fast_release * self.fast_env + (1.0 - self.fast_release) * target_gain;
        }

        // Slow envelope: smooth attack, very smooth release
        if target_gain < self.slow_env {
            self.slow_env =
                self.slow_attack * self.slow_env + (1.0 - self.slow_attack) * target_gain;
        } else {
            self.slow_env =
                self.slow_release * self.slow_env + (1.0 - self.slow_release) * target_gain;
        }

        // Use minimum of both for comprehensive protection
        self.fast_env.min(self.slow_env).clamp(0.001, 1.0)
    }

    fn reset(&mut self) {
        self.fast_env = 1.0;
        self.slow_env = 1.0;
    }
}

/// Professional True Peak Limiter
///
/// Uses lookahead to anticipate peaks and apply smooth gain reduction
/// before they arrive, guaranteeing the output never exceeds the ceiling.
pub struct Limiter {
    enabled: bool,
    #[allow(dead_code)] // Reserved for future sample-rate-dependent features
    sample_rate: f32,

    // Parameters
    /// Output ceiling in linear amplitude
    ceiling: f32,
    /// Knee threshold in linear (ceiling / knee_ratio)
    knee_threshold: f32,
    /// Knee ratio (derived from KNEE_DB)
    knee_ratio: f32,

    // Components
    /// True peak detector (4x oversampling)
    true_peak: TruePeakDetector,
    /// Peak-hold buffer for lookahead
    peak_hold: PeakHoldBuffer,
    /// Two-stage envelope follower
    envelope: TwoStageEnvelope,

    // Delay line for lookahead
    /// Delay buffer for left channel
    delay_l: Vec<f32>,
    /// Delay buffer for right channel
    delay_r: Vec<f32>,
    /// Delay buffer for gain curve
    gain_buffer: Vec<f32>,
    /// Current write position
    write_pos: usize,
    /// Lookahead in samples
    lookahead_samples: usize,

    // Metering
    /// Current gain reduction in dB
    current_gr_db: f32,
    /// Peak gain reduction in dB (with hold)
    peak_gr_db: f32,
    /// Peak hold counter
    peak_hold_counter: usize,
    /// Peak hold time in samples
    peak_hold_samples: usize,
}

impl Limiter {
    /// Create a new professional true peak limiter
    pub fn new(sample_rate: f32) -> Self {
        let lookahead_samples = (sample_rate * LOOKAHEAD_MS / 1000.0) as usize;
        let ceiling = Self::db_to_linear(DEFAULT_CEILING_DB);

        // Calculate knee parameters
        let knee_ratio = Self::db_to_linear(KNEE_DB);
        let knee_threshold = ceiling / knee_ratio;

        Self {
            enabled: true, // Always on by default for safety
            sample_rate,
            ceiling,
            knee_threshold,
            knee_ratio,
            true_peak: TruePeakDetector::new(),
            peak_hold: PeakHoldBuffer::new(lookahead_samples),
            envelope: TwoStageEnvelope::new(sample_rate),
            delay_l: vec![0.0; lookahead_samples],
            delay_r: vec![0.0; lookahead_samples],
            gain_buffer: vec![1.0; lookahead_samples],
            write_pos: 0,
            lookahead_samples,
            current_gr_db: 0.0,
            peak_gr_db: 0.0,
            peak_hold_counter: 0,
            peak_hold_samples: (sample_rate * 0.5) as usize, // 500ms hold
        }
    }

    /// Convert dB to linear amplitude
    #[inline]
    fn db_to_linear(db: f32) -> f32 {
        10.0f32.powf(db / 20.0)
    }

    /// Convert linear amplitude to dB
    #[inline]
    fn linear_to_db(linear: f32) -> f32 {
        if linear > 0.0 {
            20.0 * linear.log10()
        } else {
            -120.0
        }
    }

    /// Set the output ceiling in dB
    pub fn set_ceiling_db(&mut self, db: f32) {
        self.ceiling = Self::db_to_linear(db.clamp(-12.0, 0.0));
        self.knee_threshold = self.ceiling / self.knee_ratio;
    }

    /// Get current gain reduction in dB (for metering)
    pub fn gain_reduction_db(&self) -> f32 {
        self.current_gr_db
    }

    /// Get peak gain reduction in dB (for metering)
    pub fn peak_gain_reduction_db(&self) -> f32 {
        self.peak_gr_db
    }

    /// Calculate gain with soft knee
    ///
    /// Uses quadratic interpolation through the knee region for smooth transition.
    /// The soft knee provides gradual onset of limiting before reaching the ceiling,
    /// preventing harsh distortion at the threshold boundary.
    #[inline]
    fn calculate_gain(&self, peak: f32) -> f32 {
        if peak <= self.knee_threshold {
            // Below knee: no reduction
            1.0
        } else if peak >= self.ceiling {
            // Above ceiling: full reduction
            self.ceiling / peak
        } else {
            // In knee region: quadratic interpolation
            // Map peak to 0-1 range within knee
            let knee_range = self.ceiling - self.knee_threshold;
            let x = (peak - self.knee_threshold) / knee_range;

            // Soft knee curve: gradually reduce gain as we approach ceiling
            // At x=0 (knee_threshold): output = peak (gain = 1.0)
            // At x=1 (ceiling): output = ceiling (gain = 1.0)
            // In between: output follows a curve that stays below the linear path
            // This provides "compression" before hitting the ceiling
            //
            // Output curve: knee_threshold + x^2 * knee_range
            // (quadratic makes it curve below the linear line)
            let output = self.knee_threshold + x * x * knee_range;
            (output / peak).min(1.0)
        }
    }

    /// Process a stereo sample pair
    #[inline]
    fn process_sample(&mut self, left: f32, right: f32) -> (f32, f32) {
        // 1. Detect true peak using 4x oversampling
        let true_peak = self.true_peak.detect(left, right);

        // 2. Calculate target gain with soft knee
        let target_gain = self.calculate_gain(true_peak);

        // 3. Push to peak-hold buffer, get held peak
        let held_peak = self.peak_hold.push(true_peak);
        let held_gain = self.calculate_gain(held_peak);

        // 4. Process through two-stage envelope
        let envelope_gain = self.envelope.process(held_gain.min(target_gain));

        // 5. Read position for delayed output
        let read_pos = (self.write_pos + 1) % self.lookahead_samples;

        // 6. Get delayed samples
        let delayed_l = self.delay_l[read_pos];
        let delayed_r = self.delay_r[read_pos];

        // 7. Get anticipatory gain (calculated lookahead_samples ago)
        let output_gain = self.gain_buffer[read_pos];

        // 8. Write current samples and gain to delay buffers
        self.delay_l[self.write_pos] = left;
        self.delay_r[self.write_pos] = right;
        self.gain_buffer[self.write_pos] = envelope_gain;

        // 9. Advance write position
        self.write_pos = (self.write_pos + 1) % self.lookahead_samples;

        // 10. Apply gain to delayed signal
        let out_l = delayed_l * output_gain;
        let out_r = delayed_r * output_gain;

        // 11. Update metering
        self.current_gr_db = Self::linear_to_db(output_gain);
        if self.current_gr_db < self.peak_gr_db {
            self.peak_gr_db = self.current_gr_db;
            self.peak_hold_counter = self.peak_hold_samples;
        } else if self.peak_hold_counter > 0 {
            self.peak_hold_counter -= 1;
        } else {
            // Slow decay of peak meter
            self.peak_gr_db = self.peak_gr_db * 0.9999 + self.current_gr_db * 0.0001;
        }

        // 12. Final safety clamp (should rarely trigger with proper limiting)
        (
            out_l.clamp(-self.ceiling, self.ceiling),
            out_r.clamp(-self.ceiling, self.ceiling),
        )
    }
}

impl Effect for Limiter {
    fn process(&mut self, samples: &mut [f32]) {
        if !self.enabled {
            return;
        }

        // Process stereo pairs
        for frame in samples.chunks_exact_mut(2) {
            let (out_l, out_r) = self.process_sample(frame[0], frame[1]);
            frame[0] = out_l;
            frame[1] = out_r;
        }
    }

    fn reset(&mut self) {
        self.true_peak.reset();
        self.peak_hold.reset();
        self.envelope.reset();
        self.delay_l.fill(0.0);
        self.delay_r.fill(0.0);
        self.gain_buffer.fill(1.0);
        self.write_pos = 0;
        self.current_gr_db = 0.0;
        self.peak_gr_db = 0.0;
        self.peak_hold_counter = 0;
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.reset();
        }
    }

    fn name(&self) -> &'static str {
        "Limiter"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_limiter_creation() {
        let limiter = Limiter::new(48000.0);
        assert!(limiter.is_enabled());
        // -1.0 dBFS = ~0.891
        assert!(limiter.ceiling > 0.88 && limiter.ceiling < 0.92);
    }

    #[test]
    fn test_soft_signal_passes_through() {
        let mut limiter = Limiter::new(48000.0);

        // Prime the delay buffer with silence
        let mut silence = vec![0.0; 512];
        limiter.process(&mut silence);

        // Process low-level signal
        let mut samples: Vec<f32> = (0..512)
            .map(|i| (i as f32 * 0.01).sin() * 0.3) // Well below ceiling
            .collect();

        let original: Vec<f32> = samples.clone();
        limiter.process(&mut samples);

        // After lookahead, signal should pass through with minimal change
        // (only very slight from soft knee at low levels)
        let delay_samples = limiter.lookahead_samples * 2;
        for i in delay_samples..samples.len() {
            let diff = (samples[i] - original[i - delay_samples]).abs();
            assert!(
                diff < 0.1,
                "Sample {} differs too much: {} vs {}",
                i,
                samples[i],
                original[i - delay_samples]
            );
        }
    }

    #[test]
    fn test_loud_signal_gets_limited() {
        let mut limiter = Limiter::new(48000.0);
        let ceiling = limiter.ceiling;

        // Very loud signal
        let mut samples: Vec<f32> = vec![2.0, 2.0, -2.0, -2.0, 1.5, 1.5, -1.5, -1.5];
        samples.extend(vec![0.0; 512]); // Padding for delay

        limiter.process(&mut samples);

        // Check that output doesn't exceed ceiling
        for (i, sample) in samples.iter().enumerate() {
            assert!(
                sample.abs() <= ceiling * 1.001,
                "Sample {} at index {} exceeds ceiling {}",
                sample,
                i,
                ceiling
            );
        }
    }

    #[test]
    fn test_transient_limiting() {
        let mut limiter = Limiter::new(48000.0);
        let ceiling = limiter.ceiling;

        // Simulate a transient (sudden spike)
        let mut samples = vec![0.0; 256]; // Quiet
        samples.extend(vec![3.0, 3.0, 3.0, 3.0]); // Sudden loud transient
        samples.extend(vec![0.0; 512]); // Back to quiet

        limiter.process(&mut samples);

        // Verify no sample exceeds ceiling
        for sample in &samples {
            assert!(
                sample.abs() <= ceiling * 1.001,
                "Transient sample {} exceeds ceiling {}",
                sample,
                ceiling
            );
        }
    }

    #[test]
    fn test_gain_reduction_metering() {
        let mut limiter = Limiter::new(48000.0);

        // Process loud signal - need enough samples to get past the lookahead delay
        // Lookahead is 5ms at 48kHz = 240 samples
        let mut samples: Vec<f32> = vec![2.0; 512];
        limiter.process(&mut samples);

        // Should show gain reduction (negative dB)
        let gr_db = limiter.gain_reduction_db();
        assert!(gr_db < 0.0, "Expected gain reduction, got {} dB", gr_db);
    }

    #[test]
    fn test_ceiling_adjustment() {
        let mut limiter = Limiter::new(48000.0);

        limiter.set_ceiling_db(-3.0);
        let expected = Limiter::db_to_linear(-3.0);
        assert!((limiter.ceiling - expected).abs() < 0.001);

        limiter.set_ceiling_db(-0.1);
        let expected = Limiter::db_to_linear(-0.1);
        assert!((limiter.ceiling - expected).abs() < 0.001);
    }

    #[test]
    fn test_soft_knee() {
        let limiter = Limiter::new(48000.0);

        // Below knee threshold: gain = 1.0
        let gain_low = limiter.calculate_gain(0.5);
        assert!(
            (gain_low - 1.0).abs() < 0.001,
            "Expected gain 1.0 below knee, got {}",
            gain_low
        );

        // Above ceiling: gain = ceiling/peak
        let gain_ceiling = limiter.calculate_gain(2.0);
        let expected = limiter.ceiling / 2.0;
        assert!(
            (gain_ceiling - expected).abs() < 0.01,
            "Expected gain {}, got {}",
            expected,
            gain_ceiling
        );

        // At knee threshold: gain = 1.0
        let gain_at_knee = limiter.calculate_gain(limiter.knee_threshold);
        assert!(
            (gain_at_knee - 1.0).abs() < 0.001,
            "Expected gain 1.0 at knee threshold, got {}",
            gain_at_knee
        );

        // At ceiling: gain = 1.0 (ceiling/ceiling)
        let gain_at_ceiling = limiter.calculate_gain(limiter.ceiling);
        assert!(
            (gain_at_ceiling - 1.0).abs() < 0.001,
            "Expected gain 1.0 at ceiling, got {}",
            gain_at_ceiling
        );

        // In knee region: gain should be <= 1.0 (soft compression)
        let knee_peak = (limiter.knee_threshold + limiter.ceiling) / 2.0;
        let gain_knee = limiter.calculate_gain(knee_peak);
        assert!(
            gain_knee <= 1.0 && gain_knee > 0.9,
            "Knee gain {} should be between 0.9 and 1.0",
            gain_knee
        );
    }

    #[test]
    fn test_peak_hold_buffer() {
        let mut buffer = PeakHoldBuffer::new(3);

        // Push increasing values
        assert_eq!(buffer.push(0.5), 0.5);
        assert_eq!(buffer.push(0.7), 0.7);
        assert_eq!(buffer.push(0.6), 0.7); // Still 0.7 (held)

        // After hold time, should see decrease
        assert_eq!(buffer.push(0.4), 0.7); // 0.7 not expired yet
        assert_eq!(buffer.push(0.3), 0.6); // 0.7 expired, next is 0.6
        assert_eq!(buffer.push(0.2), 0.4); // Continue decay
    }

    #[test]
    fn test_true_peak_detector() {
        let mut detector = TruePeakDetector::new();

        // Test with a simple signal
        let peak1 = detector.detect(0.5, 0.5);
        assert!(peak1 > 0.0, "Should detect some peak");

        // Test with larger signal
        let peak2 = detector.detect(0.9, 0.9);
        assert!(peak2 > peak1, "Larger input should give larger peak");
    }
}
