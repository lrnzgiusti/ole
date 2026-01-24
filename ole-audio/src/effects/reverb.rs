//! Freeverb-style reverb effect
//!
//! Uses parallel comb filters and series allpass filters for
//! rich, natural-sounding reverberation.

use super::Effect;

/// Comb filter delay times in samples at 44.1kHz (from Freeverb)
const COMB_TUNINGS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];

/// Allpass filter delay times in samples at 44.1kHz
const ALLPASS_TUNINGS: [usize; 4] = [556, 441, 341, 225];

/// Stereo spread in samples
const STEREO_SPREAD: usize = 23;

/// Lowpass-feedback comb filter
struct CombFilter {
    buffer: Vec<f32>,
    buffer_size: usize,
    index: usize,
    filter_store: f32,
}

impl CombFilter {
    fn new(size: usize) -> Self {
        Self {
            buffer: vec![0.0; size],
            buffer_size: size,
            index: 0,
            filter_store: 0.0,
        }
    }

    fn process(&mut self, input: f32, feedback: f32, damping: f32) -> f32 {
        let output = self.buffer[self.index];

        // Lowpass filter in feedback path (damping)
        self.filter_store = output * (1.0 - damping) + self.filter_store * damping;

        // Write input + filtered feedback to buffer
        self.buffer[self.index] = input + self.filter_store * feedback;

        // Advance index
        self.index = (self.index + 1) % self.buffer_size;

        output
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.filter_store = 0.0;
        self.index = 0;
    }
}

/// Schroeder allpass filter
struct AllpassFilter {
    buffer: Vec<f32>,
    buffer_size: usize,
    index: usize,
}

impl AllpassFilter {
    fn new(size: usize) -> Self {
        Self {
            buffer: vec![0.0; size],
            buffer_size: size,
            index: 0,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let buffered = self.buffer[self.index];
        let output = -input + buffered;

        // Feedback coefficient of 0.5 (standard for allpass diffusion)
        self.buffer[self.index] = input + buffered * 0.5;

        self.index = (self.index + 1) % self.buffer_size;

        output
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.index = 0;
    }
}

/// Freeverb-style stereo reverb effect
pub struct Reverb {
    // Left channel filters
    comb_l: [CombFilter; 8],
    allpass_l: [AllpassFilter; 4],

    // Right channel filters (with stereo spread)
    comb_r: [CombFilter; 8],
    allpass_r: [AllpassFilter; 4],

    // Parameters
    room_size: f32, // 0.0 - 1.0
    damping: f32,   // 0.0 - 1.0
    wet: f32,       // 0.0 - 1.0
    dry: f32,       // 0.0 - 1.0
    width: f32,     // Stereo width 0.0 - 1.0

    // Current level preset (1-5)
    level: u8,

    enabled: bool,

    // Cached computed values (updated on parameter change to avoid per-sample calculation)
    cached_feedback: f32,
    cached_wet1: f32,
    cached_wet2: f32,

    // Wet envelope for click-free enable/disable
    wet_target: f32,
    wet_current: f32,
}

impl Reverb {
    /// Create a new reverb effect
    pub fn new(sample_rate: u32) -> Self {
        // Scale tunings for sample rate
        let scale = sample_rate as f32 / 44100.0;

        // Create left channel filters
        let comb_l =
            std::array::from_fn(|i| CombFilter::new((COMB_TUNINGS[i] as f32 * scale) as usize));
        let allpass_l = std::array::from_fn(|i| {
            AllpassFilter::new((ALLPASS_TUNINGS[i] as f32 * scale) as usize)
        });

        // Create right channel filters with stereo spread
        let spread = (STEREO_SPREAD as f32 * scale) as usize;
        let comb_r = std::array::from_fn(|i| {
            CombFilter::new((COMB_TUNINGS[i] as f32 * scale) as usize + spread)
        });
        let allpass_r = std::array::from_fn(|i| {
            AllpassFilter::new((ALLPASS_TUNINGS[i] as f32 * scale) as usize + spread)
        });

        let room_size = 0.5;
        let wet = 0.3;
        let width = 1.0;

        Self {
            comb_l,
            allpass_l,
            comb_r,
            allpass_r,
            room_size,
            damping: 0.5,
            wet,
            dry: 0.7,
            width,
            level: 0,
            enabled: false,
            // Pre-compute cached values
            cached_feedback: room_size * 0.24 + 0.6,
            cached_wet1: wet * (width * 0.5 + 0.5),
            cached_wet2: wet * ((1.0 - width) * 0.5),
            wet_target: 0.0,
            wet_current: 0.0,
        }
    }

    /// Wet envelope smoothing coefficient (~10ms at 48kHz)
    const WET_SMOOTH_COEFF: f32 = 0.9995;

    /// Update cached computed values (called when parameters change)
    #[inline]
    fn update_cached(&mut self) {
        self.cached_feedback = self.room_size * 0.24 + 0.6;
        self.cached_wet1 = self.wet * (self.width * 0.5 + 0.5);
        self.cached_wet2 = self.wet * ((1.0 - self.width) * 0.5);
    }

    /// Set room size (0.0 - 1.0)
    pub fn set_room_size(&mut self, size: f32) {
        self.room_size = size.clamp(0.0, 1.0);
        self.update_cached();
    }

    /// Get room size
    pub fn room_size(&self) -> f32 {
        self.room_size
    }

    /// Set damping (0.0 - 1.0)
    pub fn set_damping(&mut self, damping: f32) {
        self.damping = damping.clamp(0.0, 1.0);
    }

    /// Get damping
    pub fn damping(&self) -> f32 {
        self.damping
    }

    /// Set wet level (0.0 - 1.0)
    pub fn set_wet(&mut self, wet: f32) {
        self.wet = wet.clamp(0.0, 1.0);
        self.update_cached();
    }

    /// Get wet level
    pub fn wet(&self) -> f32 {
        self.wet
    }

    /// Set dry level (0.0 - 1.0)
    pub fn set_dry(&mut self, dry: f32) {
        self.dry = dry.clamp(0.0, 1.0);
    }

    /// Set stereo width (0.0 - 1.0)
    pub fn set_width(&mut self, width: f32) {
        self.width = width.clamp(0.0, 1.0);
        self.update_cached();
    }

    /// Set reverb level preset (1-5)
    ///
    /// - Level 1: Small room - subtle ambience
    /// - Level 2: Medium room - light reverb
    /// - Level 3: Large room - noticeable but clean
    /// - Level 4: Hall - spacious
    /// - Level 5: Cathedral - lush, long tail
    pub fn set_level(&mut self, level: u8) {
        let level = level.clamp(1, 5);
        self.level = level;

        // Softer presets with more damping and less wet signal
        let (room_size, damping, wet) = match level {
            1 => (0.3, 0.7, 0.08),  // Small room - very subtle
            2 => (0.45, 0.6, 0.12), // Medium room - light
            3 => (0.6, 0.5, 0.16),  // Large room - moderate
            4 => (0.75, 0.4, 0.20), // Hall - spacious
            5 => (0.85, 0.3, 0.25), // Cathedral - lush
            _ => (0.5, 0.5, 0.15),
        };

        self.room_size = room_size;
        self.damping = damping;
        self.wet = wet;
        // Proportional dry: reduce dry as wet increases to prevent > 1.0 sum
        // This keeps some original signal while allowing full reverb effect
        self.dry = 1.0 - wet * 0.5;
        self.update_cached();

        // Auto-enable when setting a level
        self.enabled = true;
    }

    /// Get current level preset
    pub fn level(&self) -> u8 {
        self.level
    }

    /// Process a stereo sample pair
    fn process_sample(&mut self, left: f32, right: f32) -> (f32, f32) {
        // Attenuate input to prevent buildup
        let input = (left + right) * 0.25;

        // Use cached feedback (updated on parameter change, not per-sample)
        let feedback = self.cached_feedback;

        // Process through parallel comb filters
        let mut out_l = 0.0;
        let mut out_r = 0.0;

        for comb in &mut self.comb_l {
            out_l += comb.process(input, feedback, self.damping);
        }
        for comb in &mut self.comb_r {
            out_r += comb.process(input, feedback, self.damping);
        }

        // Scale down comb output (8 filters summed)
        out_l *= 0.125;
        out_r *= 0.125;

        // Process through series allpass filters (these are unity gain)
        for allpass in &mut self.allpass_l {
            out_l = allpass.process(out_l);
        }
        for allpass in &mut self.allpass_r {
            out_r = allpass.process(out_r);
        }

        // Use cached wet values (updated on parameter change, not per-sample)
        let wet1 = self.cached_wet1;
        let wet2 = self.cached_wet2;

        // Normalize: ensure total gain doesn't exceed 1.0
        // With width=1.0: wet1=wet, wet2=0, so total_wet = wet
        // We scale dry down so that dry + total_wet <= 1.0
        let total_wet = wet1 + wet2;
        let dry_norm = self.dry * (1.0 - total_wet).max(0.0);

        // Final mix - wet reverb + normalized dry original
        let final_l = out_l * wet1 + out_r * wet2 + left * dry_norm;
        let final_r = out_r * wet1 + out_l * wet2 + right * dry_norm;

        // Soft clip to prevent any remaining distortion
        (soft_clip(final_l), soft_clip(final_r))
    }
}

/// Soft clipper to prevent harsh distortion
fn soft_clip(x: f32) -> f32 {
    if x > 1.0 {
        1.0 - 1.0 / (1.0 + (x - 1.0) * 2.0)
    } else if x < -1.0 {
        -1.0 + 1.0 / (1.0 + (-x - 1.0) * 2.0)
    } else {
        x
    }
}

impl Effect for Reverb {
    fn process(&mut self, samples: &mut [f32]) {
        // Skip processing only if fully disabled and envelope has settled
        if !self.enabled && self.wet_current < 0.0001 {
            return;
        }

        // Process stereo pairs
        for chunk in samples.chunks_mut(2) {
            if chunk.len() == 2 {
                // Smooth wet envelope toward target
                self.wet_current = Self::WET_SMOOTH_COEFF * self.wet_current
                    + (1.0 - Self::WET_SMOOTH_COEFF) * self.wet_target;

                // Process through reverb
                let (wet_l, wet_r) = self.process_sample(chunk[0], chunk[1]);

                // Crossfade between dry and wet based on envelope
                // process_sample already mixes dry/wet, so we interpolate the full output
                let dry_l = chunk[0];
                let dry_r = chunk[1];
                chunk[0] = dry_l * (1.0 - self.wet_current) + wet_l * self.wet_current;
                chunk[1] = dry_r * (1.0 - self.wet_current) + wet_r * self.wet_current;
            }
        }
    }

    fn reset(&mut self) {
        for comb in &mut self.comb_l {
            comb.reset();
        }
        for comb in &mut self.comb_r {
            comb.reset();
        }
        for allpass in &mut self.allpass_l {
            allpass.reset();
        }
        for allpass in &mut self.allpass_r {
            allpass.reset();
        }
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.wet_target = if enabled { 1.0 } else { 0.0 };
        // Note: don't reset on disable - let reverb tails naturally fade out
    }

    fn name(&self) -> &'static str {
        "Reverb"
    }
}
