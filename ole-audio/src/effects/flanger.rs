//! Flanger effect - sweeping comb filter for jet-plane swoosh
//!
//! Classic DJ effect that creates a sweeping metallic sound by mixing
//! the signal with a slightly delayed copy that varies over time.

use super::Effect;
use std::f32::consts::PI;

/// Flanger effect with LFO modulation
pub struct Flanger {
    enabled: bool,
    sample_rate: f32,

    /// LFO rate in Hz (0.05 - 5.0)
    rate: f32,

    /// Modulation depth (0.0 - 1.0)
    depth: f32,

    /// Feedback amount (-0.95 to 0.95)
    feedback: f32,

    /// Wet/dry mix (0.0 - 1.0)
    mix: f32,

    /// Base delay in ms (0.1 - 10.0)
    base_delay_ms: f32,

    /// LFO phase (0.0 - 1.0)
    lfo_phase: f32,

    /// LFO phase increment per sample
    lfo_inc: f32,

    /// Delay buffer (stereo interleaved)
    delay_buffer: Vec<f32>,

    /// Buffer write position
    write_pos: usize,

    /// Feedback state (stereo)
    feedback_l: f32,
    feedback_r: f32,

    /// Wet envelope for click-free enable/disable
    wet_target: f32,
    wet_current: f32,
}

impl Flanger {
    /// Maximum delay in samples (for 10ms at 192kHz)
    const MAX_DELAY_SAMPLES: usize = 2048;

    /// Wet envelope smoothing coefficient
    const WET_SMOOTH_COEFF: f32 = 0.9995;

    /// Create a new flanger effect
    pub fn new(sample_rate: f32) -> Self {
        let rate = 0.5; // 0.5 Hz default
        let lfo_inc = rate / sample_rate;

        Self {
            enabled: false,
            sample_rate,
            rate,
            depth: 0.7,
            feedback: 0.5,
            mix: 0.5,
            base_delay_ms: 1.0,
            lfo_phase: 0.0,
            lfo_inc,
            delay_buffer: vec![0.0; Self::MAX_DELAY_SAMPLES * 2],
            write_pos: 0,
            feedback_l: 0.0,
            feedback_r: 0.0,
            wet_target: 0.0,
            wet_current: 0.0,
        }
    }

    /// Set LFO rate in Hz (0.05 - 5.0)
    pub fn set_rate(&mut self, rate: f32) {
        self.rate = rate.clamp(0.05, 5.0);
        self.lfo_inc = self.rate / self.sample_rate;
    }

    /// Get LFO rate
    pub fn rate(&self) -> f32 {
        self.rate
    }

    /// Set modulation depth (0.0 - 1.0)
    pub fn set_depth(&mut self, depth: f32) {
        self.depth = depth.clamp(0.0, 1.0);
    }

    /// Get depth
    pub fn depth(&self) -> f32 {
        self.depth
    }

    /// Set feedback amount (-0.95 to 0.95)
    pub fn set_feedback(&mut self, feedback: f32) {
        self.feedback = feedback.clamp(-0.95, 0.95);
    }

    /// Get feedback
    pub fn feedback(&self) -> f32 {
        self.feedback
    }

    /// Set wet/dry mix (0.0 - 1.0)
    pub fn set_mix(&mut self, mix: f32) {
        self.mix = mix.clamp(0.0, 1.0);
    }

    /// Get mix
    pub fn mix(&self) -> f32 {
        self.mix
    }

    /// Set base delay in ms (0.1 - 10.0)
    pub fn set_base_delay(&mut self, delay_ms: f32) {
        self.base_delay_ms = delay_ms.clamp(0.1, 10.0);
    }

    /// Get base delay
    pub fn base_delay(&self) -> f32 {
        self.base_delay_ms
    }

    /// Read from delay buffer with linear interpolation
    #[inline]
    fn read_delay(&self, delay_samples: f32, is_right: bool) -> f32 {
        let channel_offset = if is_right { 1 } else { 0 };
        let max_delay = Self::MAX_DELAY_SAMPLES as f32 - 2.0;
        let delay = delay_samples.clamp(1.0, max_delay);

        let read_pos = (self.write_pos as f32 - delay).rem_euclid(Self::MAX_DELAY_SAMPLES as f32);
        let pos_int = (read_pos as usize) % Self::MAX_DELAY_SAMPLES;
        let frac = read_pos.fract();

        let i0 = pos_int * 2 + channel_offset;
        let i1 = ((pos_int + 1) % Self::MAX_DELAY_SAMPLES) * 2 + channel_offset;

        self.delay_buffer[i0] * (1.0 - frac) + self.delay_buffer[i1] * frac
    }

    /// Soft saturation for feedback path
    #[inline]
    fn soft_saturate(x: f32) -> f32 {
        let x2 = x * x;
        x * (27.0 + x2) / (27.0 + 9.0 * x2)
    }

    /// Soft clipper to prevent output from exceeding ceiling
    #[inline]
    fn soft_clip(x: f32) -> f32 {
        if x > 1.0 {
            1.0 - 1.0 / (1.0 + (x - 1.0) * 2.0)
        } else if x < -1.0 {
            -1.0 + 1.0 / (1.0 + (-x - 1.0) * 2.0)
        } else {
            x
        }
    }
}

impl Effect for Flanger {
    fn process(&mut self, samples: &mut [f32]) {
        // Skip if fully disabled and envelope settled
        if !self.enabled && self.wet_current < 0.0001 {
            return;
        }

        // Calculate delay range in samples
        let base_delay_samples = (self.base_delay_ms / 1000.0) * self.sample_rate;
        let max_sweep = base_delay_samples * 2.0; // Sweep up to 2x base delay

        for frame in samples.chunks_mut(2) {
            if frame.len() < 2 {
                continue;
            }

            // Smooth wet envelope
            self.wet_current = Self::WET_SMOOTH_COEFF * self.wet_current
                + (1.0 - Self::WET_SMOOTH_COEFF) * self.wet_target;

            // Calculate LFO value (sine wave, 0.0 - 1.0)
            let lfo = (self.lfo_phase * 2.0 * PI).sin() * 0.5 + 0.5;
            self.lfo_phase += self.lfo_inc;
            if self.lfo_phase >= 1.0 {
                self.lfo_phase -= 1.0;
            }

            // Calculate modulated delay
            let delay_samples = base_delay_samples + lfo * max_sweep * self.depth;

            // Read delayed signal
            let delayed_l = self.read_delay(delay_samples, false);
            let delayed_r = self.read_delay(delay_samples, true);

            // Calculate input with feedback
            let input_l = frame[0] + Self::soft_saturate(self.feedback_l * self.feedback);
            let input_r = frame[1] + Self::soft_saturate(self.feedback_r * self.feedback);

            // Write to delay buffer
            let write_idx = self.write_pos * 2;
            self.delay_buffer[write_idx] = input_l;
            self.delay_buffer[write_idx + 1] = input_r;
            self.write_pos = (self.write_pos + 1) % Self::MAX_DELAY_SAMPLES;

            // Update feedback
            self.feedback_l = delayed_l;
            self.feedback_r = delayed_r;

            // Mix dry and wet with envelope, soft clip to prevent energy accumulation
            let effective_mix = self.mix * self.wet_current;
            frame[0] =
                Self::soft_clip(frame[0] * (1.0 - effective_mix) + delayed_l * effective_mix);
            frame[1] =
                Self::soft_clip(frame[1] * (1.0 - effective_mix) + delayed_r * effective_mix);
        }
    }

    fn reset(&mut self) {
        self.delay_buffer.fill(0.0);
        self.write_pos = 0;
        self.lfo_phase = 0.0;
        self.feedback_l = 0.0;
        self.feedback_r = 0.0;
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.wet_target = if enabled { 1.0 } else { 0.0 };
    }

    fn name(&self) -> &'static str {
        "Flanger"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flanger_creation() {
        let flanger = Flanger::new(48000.0);
        assert!(!flanger.is_enabled());
        assert_eq!(flanger.rate(), 0.5);
    }

    #[test]
    fn test_flanger_parameter_clamping() {
        let mut flanger = Flanger::new(48000.0);

        flanger.set_rate(10.0);
        assert_eq!(flanger.rate(), 5.0);

        flanger.set_feedback(2.0);
        assert_eq!(flanger.feedback(), 0.95);

        flanger.set_feedback(-2.0);
        assert_eq!(flanger.feedback(), -0.95);
    }

    #[test]
    fn test_flanger_processes_audio() {
        let mut flanger = Flanger::new(48000.0);
        flanger.set_enabled(true);
        flanger.wet_current = 1.0; // Force wet for test

        let mut samples = vec![0.5, 0.5, 0.3, 0.3, 0.1, 0.1];
        flanger.process(&mut samples);

        // Output should be modified (delayed signal mixed in)
        // Due to delay, first samples might still be close to original
    }
}
