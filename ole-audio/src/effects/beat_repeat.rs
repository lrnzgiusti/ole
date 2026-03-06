//! Beat Repeat / Roll effect - captures a slice of audio and loops it
//!
//! Classic DJ effect that freezes a rhythmic segment of audio and repeats it
//! in sync with the BPM. Useful for build-ups, fills, and glitch effects.

use super::gate::GateDivision;
use super::Effect;

/// Beat Repeat effect that captures and loops audio segments
pub struct BeatRepeat {
    enabled: bool,
    sample_rate: f32,

    /// Rhythmic division for segment length
    division: GateDivision,

    /// Volume decay per repetition (0.0 - 1.0)
    decay: f32,

    /// Wet/dry mix (0.0 - 1.0)
    mix: f32,

    /// Current BPM for segment calculation
    bpm: f32,

    /// Circular capture buffer (stereo interleaved)
    capture_buffer: Vec<f32>,

    /// Write position in capture buffer (in stereo frames)
    write_pos: usize,

    /// Read position in capture buffer (in stereo samples)
    read_pos: usize,

    /// Length of frozen segment in stereo samples (frames * 2)
    segment_len: usize,

    /// Start of frozen segment in capture buffer (stereo sample index)
    segment_start: usize,

    /// Whether we're in repeat mode
    triggered: bool,

    /// How many repeats have completed (for decay)
    repeat_count: u32,

    /// Pre-computed gain for current repeat cycle
    current_gain: f32,

    /// Counts samples since trigger for auto-release
    auto_release_counter: usize,

    /// Auto-release threshold in stereo samples (8 beats)
    auto_release_threshold: usize,

    /// Wet envelope for click-free enable/disable
    wet_target: f32,
    wet_current: f32,
}

/// Number of stereo frames in the capture buffer (2 seconds at 48kHz)
const BUFFER_FRAMES: usize = 96000;

impl BeatRepeat {
    /// Wet envelope smoothing coefficient
    const WET_SMOOTH_COEFF: f32 = 0.9995;

    /// Create a new beat repeat effect
    pub fn new(sample_rate: f32) -> Self {
        let bpm = 120.0;
        let division = GateDivision::default();
        let segment_len = Self::calc_segment_len(sample_rate, bpm, division);
        let auto_release_threshold = Self::calc_auto_release(sample_rate, bpm);

        Self {
            enabled: false,
            sample_rate,
            division,
            decay: 0.0,
            mix: 1.0,
            bpm,
            capture_buffer: vec![0.0; BUFFER_FRAMES * 2],
            write_pos: 0,
            read_pos: 0,
            segment_len,
            segment_start: 0,
            triggered: false,
            repeat_count: 0,
            current_gain: 1.0,
            auto_release_counter: 0,
            auto_release_threshold,
            wet_target: 0.0,
            wet_current: 0.0,
        }
    }

    /// Calculate segment length in stereo samples from BPM and division
    fn calc_segment_len(sample_rate: f32, bpm: f32, division: GateDivision) -> usize {
        let beat_duration_secs = 60.0 / bpm;
        let segment_secs = beat_duration_secs * division.beats();
        let segment_frames = (segment_secs * sample_rate) as usize;
        // Stereo samples, clamped to buffer size
        (segment_frames * 2).clamp(2, BUFFER_FRAMES * 2)
    }

    /// Calculate auto-release threshold (8 beats in stereo samples)
    fn calc_auto_release(sample_rate: f32, bpm: f32) -> usize {
        let beat_duration_secs = 60.0 / bpm;
        let eight_beats_secs = beat_duration_secs * 8.0;
        let frames = (eight_beats_secs * sample_rate) as usize;
        frames * 2 // stereo samples
    }

    /// Set rhythmic division
    pub fn set_division(&mut self, division: GateDivision) {
        self.division = division;
        self.segment_len = Self::calc_segment_len(self.sample_rate, self.bpm, self.division);
    }

    /// Get current division
    pub fn division(&self) -> GateDivision {
        self.division
    }

    /// Set volume decay per repetition (0.0 - 1.0)
    pub fn set_decay(&mut self, decay: f32) {
        self.decay = decay.clamp(0.0, 1.0);
    }

    /// Get decay
    pub fn decay(&self) -> f32 {
        self.decay
    }

    /// Set wet/dry mix (0.0 - 1.0)
    pub fn set_mix(&mut self, mix: f32) {
        self.mix = mix.clamp(0.0, 1.0);
    }

    /// Get mix
    pub fn mix(&self) -> f32 {
        self.mix
    }

    /// Set BPM (updates segment length and auto-release calculations)
    pub fn set_bpm(&mut self, bpm: f32) {
        if !bpm.is_finite() {
            return;
        }
        self.bpm = bpm.clamp(20.0, 300.0);
        // Don't update segment_len while triggered — changing it mid-loop
        // corrupts wrap detection since segment_start was set with the old length
        if !self.triggered {
            self.segment_len = Self::calc_segment_len(self.sample_rate, self.bpm, self.division);
        }
        self.auto_release_threshold = Self::calc_auto_release(self.sample_rate, self.bpm);
    }

    /// Get current BPM
    pub fn bpm(&self) -> f32 {
        self.bpm
    }

    /// Trigger the beat repeat. If already triggered, release instead.
    pub fn trigger(&mut self) {
        if self.triggered {
            self.release();
            return;
        }

        // Calculate segment length from current BPM and division
        self.segment_len = Self::calc_segment_len(self.sample_rate, self.bpm, self.division);

        let buffer_len = BUFFER_FRAMES * 2;

        // Set read_pos to write_pos - segment_len (wrap around circular buffer)
        let write_sample_pos = self.write_pos * 2;
        if write_sample_pos >= self.segment_len {
            self.segment_start = write_sample_pos - self.segment_len;
        } else {
            self.segment_start = buffer_len - (self.segment_len - write_sample_pos);
        }
        self.read_pos = self.segment_start;

        self.triggered = true;
        self.repeat_count = 0;
        self.current_gain = 1.0;
        self.auto_release_counter = 0;
    }

    /// Release the beat repeat (stop looping)
    pub fn release(&mut self) {
        self.triggered = false;
    }

    /// Check if currently triggered
    pub fn is_triggered(&self) -> bool {
        self.triggered
    }

    /// Compute decay gain for given repeat count
    #[inline]
    fn decay_gain(&self) -> f32 {
        if self.decay <= 0.0 {
            return 1.0;
        }
        (1.0 - self.decay).powf(self.repeat_count as f32)
    }

    /// Advance read position by one stereo pair, wrapping within segment
    #[inline]
    fn advance_read_pos(&mut self) {
        let buffer_len = BUFFER_FRAMES * 2;
        self.read_pos = (self.read_pos + 2) % buffer_len;

        // Check if we've reached the end of the segment
        let segment_end = (self.segment_start + self.segment_len) % buffer_len;

        let wrapped = if self.segment_start <= segment_end {
            // Segment doesn't wrap around buffer (includes exact-length case)
            self.read_pos >= segment_end
        } else {
            // Segment wraps around buffer end
            self.read_pos >= segment_end && self.read_pos < self.segment_start
        };

        if wrapped {
            self.read_pos = self.segment_start;
            self.repeat_count += 1;
            self.current_gain = self.decay_gain();
        }
    }
}

impl Effect for BeatRepeat {
    fn process(&mut self, samples: &mut [f32]) {
        // Skip if fully disabled and envelope settled
        if !self.enabled && self.wet_current < 0.0001 && !self.triggered {
            return;
        }

        let buffer_len = BUFFER_FRAMES * 2;

        for frame in samples.chunks_mut(2) {
            if frame.len() < 2 {
                continue;
            }

            // Smooth wet envelope
            self.wet_current = Self::WET_SMOOTH_COEFF * self.wet_current
                + (1.0 - Self::WET_SMOOTH_COEFF) * self.wet_target;

            let dry_l = frame[0];
            let dry_r = frame[1];

            // Always write dry input to capture buffer at write_pos (circular)
            let write_idx = self.write_pos * 2;
            self.capture_buffer[write_idx] = dry_l;
            self.capture_buffer[write_idx + 1] = dry_r;
            self.write_pos = (self.write_pos + 1) % BUFFER_FRAMES;

            if self.triggered {
                // Read from capture buffer at read_pos
                let captured_l = self.capture_buffer[self.read_pos % buffer_len];
                let captured_r = self.capture_buffer[(self.read_pos + 1) % buffer_len];

                let gain = self.current_gain;

                // Advance read position (handles wrap and repeat_count)
                self.advance_read_pos();

                // Mix: output = dry * (1 - effective_mix) + captured * gain * effective_mix
                let effective_mix = self.mix * self.wet_current;
                frame[0] = dry_l * (1.0 - effective_mix) + captured_l * gain * effective_mix;
                frame[1] = dry_r * (1.0 - effective_mix) + captured_r * gain * effective_mix;

                // Auto-release after 8 beats
                self.auto_release_counter += 2; // stereo samples
                if self.auto_release_counter >= self.auto_release_threshold {
                    self.release();
                }
            }
            // If not triggered, output is unchanged (dry passthrough)
        }
    }

    fn reset(&mut self) {
        self.capture_buffer.fill(0.0);
        self.write_pos = 0;
        self.read_pos = 0;
        self.segment_start = 0;
        self.segment_len = Self::calc_segment_len(self.sample_rate, self.bpm, self.division);
        self.triggered = false;
        self.repeat_count = 0;
        self.current_gain = 1.0;
        self.auto_release_counter = 0;
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.wet_target = if enabled { 1.0 } else { 0.0 };
        if !enabled {
            self.triggered = false;
        }
    }

    fn name(&self) -> &'static str {
        "Beat Repeat"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_beat_repeat_creation() {
        let br = BeatRepeat::new(48000.0);
        assert!(!br.is_enabled());
        assert!(!br.is_triggered());
        assert_eq!(br.decay(), 0.0);
        assert_eq!(br.mix(), 1.0);
        assert_eq!(br.bpm(), 120.0);
        assert_eq!(br.division(), GateDivision::Eighth);
    }

    #[test]
    fn test_parameter_clamping() {
        let mut br = BeatRepeat::new(48000.0);

        br.set_decay(2.0);
        assert_eq!(br.decay(), 1.0);

        br.set_decay(-1.0);
        assert_eq!(br.decay(), 0.0);

        br.set_mix(5.0);
        assert_eq!(br.mix(), 1.0);

        br.set_mix(-1.0);
        assert_eq!(br.mix(), 0.0);

        br.set_bpm(500.0);
        assert_eq!(br.bpm(), 300.0);

        br.set_bpm(1.0);
        assert_eq!(br.bpm(), 20.0);
    }

    #[test]
    fn test_trigger_toggle() {
        let mut br = BeatRepeat::new(48000.0);
        br.set_enabled(true);
        br.wet_current = 1.0;

        // First trigger starts repeat
        br.trigger();
        assert!(br.is_triggered());

        // Second trigger releases
        br.trigger();
        assert!(!br.is_triggered());
    }

    #[test]
    fn test_release() {
        let mut br = BeatRepeat::new(48000.0);
        br.set_enabled(true);
        br.wet_current = 1.0;

        br.trigger();
        assert!(br.is_triggered());

        br.release();
        assert!(!br.is_triggered());
    }

    #[test]
    fn test_passthrough_when_not_triggered() {
        let mut br = BeatRepeat::new(48000.0);
        br.set_enabled(true);
        br.wet_current = 1.0;

        let mut samples = vec![0.5, -0.5, 0.3, -0.3];
        let original = samples.clone();
        br.process(&mut samples);

        // Not triggered, output should equal input
        assert_eq!(samples, original);
    }

    #[test]
    fn test_segment_len_calculation() {
        let sample_rate = 48000.0;
        let bpm = 120.0;

        // At 120 BPM, one beat = 0.5s = 24000 frames = 48000 stereo samples
        // Eighth note = 0.5 beats = 0.25s = 12000 frames = 24000 stereo samples
        let len = BeatRepeat::calc_segment_len(sample_rate, bpm, GateDivision::Eighth);
        assert_eq!(len, 24000);

        // Quarter note = 1 beat = 0.5s = 24000 frames = 48000 stereo samples
        let len = BeatRepeat::calc_segment_len(sample_rate, bpm, GateDivision::Quarter);
        assert_eq!(len, 48000);

        // Sixteenth = 0.25 beats = 0.125s = 6000 frames = 12000 stereo samples
        let len = BeatRepeat::calc_segment_len(sample_rate, bpm, GateDivision::Sixteenth);
        assert_eq!(len, 12000);
    }

    #[test]
    fn test_triggered_repeats_audio() {
        let mut br = BeatRepeat::new(48000.0);
        br.set_enabled(true);
        br.wet_current = 1.0;
        br.set_division(GateDivision::ThirtySecond);

        // Fill capture buffer with known pattern first
        let segment_len =
            BeatRepeat::calc_segment_len(48000.0, 120.0, GateDivision::ThirtySecond);
        let segment_frames = segment_len / 2;

        // Write enough audio to fill at least one segment
        let mut fill = vec![0.0; segment_frames * 2];
        for i in 0..segment_frames {
            fill[i * 2] = (i as f32) / (segment_frames as f32);
            fill[i * 2 + 1] = -(i as f32) / (segment_frames as f32);
        }
        br.process(&mut fill);

        // Now trigger
        br.trigger();
        assert!(br.is_triggered());

        // Process more audio - should be repeating the captured segment
        let mut output = vec![0.0; 128];
        br.process(&mut output);

        // Output should not be all zeros (we're repeating captured audio)
        let has_nonzero = output.iter().any(|&s| s.abs() > 0.0001);
        assert!(has_nonzero, "Triggered beat repeat should produce non-zero output");
    }

    #[test]
    fn test_decay_reduces_volume() {
        let mut br = BeatRepeat::new(48000.0);
        br.set_decay(0.5);

        // repeat_count = 0 -> gain = 1.0
        br.repeat_count = 0;
        let g0 = br.decay_gain();
        assert!((g0 - 1.0).abs() < 0.001);

        // repeat_count = 1 -> gain = 0.5
        br.repeat_count = 1;
        let g1 = br.decay_gain();
        assert!((g1 - 0.5).abs() < 0.001);

        // repeat_count = 2 -> gain = 0.25
        br.repeat_count = 2;
        let g2 = br.decay_gain();
        assert!((g2 - 0.25).abs() < 0.001);
    }

    #[test]
    fn test_auto_release() {
        let mut br = BeatRepeat::new(48000.0);
        br.set_enabled(true);
        br.wet_current = 1.0;
        br.set_bpm(120.0);

        // Fill buffer first
        let mut fill = vec![0.1; 48000];
        br.process(&mut fill);

        br.trigger();
        assert!(br.is_triggered());

        // Process 8 beats worth of audio
        // At 120 BPM, 8 beats = 4 seconds = 192000 frames = 384000 stereo samples
        let eight_beats_samples = BeatRepeat::calc_auto_release(48000.0, 120.0);
        let mut buf = vec![0.1; eight_beats_samples];
        br.process(&mut buf);

        // Should have auto-released
        assert!(!br.is_triggered());
    }

    #[test]
    fn test_set_enabled_releases_trigger() {
        let mut br = BeatRepeat::new(48000.0);
        br.set_enabled(true);
        br.wet_current = 1.0;

        // Fill buffer and trigger
        let mut fill = vec![0.1; 4800];
        br.process(&mut fill);
        br.trigger();
        assert!(br.is_triggered());

        // Disabling should release
        br.set_enabled(false);
        assert!(!br.is_triggered());
    }

    #[test]
    fn test_name() {
        let br = BeatRepeat::new(48000.0);
        assert_eq!(br.name(), "Beat Repeat");
    }

    #[test]
    fn test_reset() {
        let mut br = BeatRepeat::new(48000.0);
        br.set_enabled(true);
        br.wet_current = 1.0;

        // Fill and trigger
        let mut fill = vec![0.5; 4800];
        br.process(&mut fill);
        br.trigger();

        // Process some
        let mut buf = vec![0.1; 480];
        br.process(&mut buf);

        // Reset
        br.reset();
        assert!(!br.is_triggered());
        assert_eq!(br.write_pos, 0);
        assert_eq!(br.read_pos, 0);
        assert_eq!(br.repeat_count, 0);
    }
}
