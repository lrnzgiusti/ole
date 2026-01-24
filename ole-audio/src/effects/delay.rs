//! State-of-the-art delay/echo effect with Lagrange interpolation.
//!
//! Features:
//! - 4-point Lagrange interpolation for sub-sample accuracy
//! - Optional tape-style modulation (wow/flutter)
//! - Highpass feedback filter (prevents mud buildup)
//! - Soft-knee saturation on feedback path
//! - BPM-synced delay times

use super::Effect;
use std::f32::consts::PI;

/// Maximum delay time in seconds
const MAX_DELAY_SECS: f32 = 2.0;

/// Delay interpolation mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum DelayInterpolation {
    /// No interpolation (integer samples only, lowest CPU)
    None,
    /// Linear interpolation (good balance)
    Linear,
    /// 4-point Lagrange cubic interpolation (highest quality)
    #[default]
    Lagrange,
}

/// Delay modulation mode for tape character
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum DelayModulation {
    /// No modulation (clean digital delay)
    #[default]
    Off,
    /// Subtle tape wow (slow pitch variation)
    Subtle,
    /// Classic tape modulation
    Classic,
    /// Heavy warped tape effect
    Heavy,
}

impl DelayModulation {
    /// Get modulation depth in samples
    fn depth(self) -> f32 {
        match self {
            DelayModulation::Off => 0.0,
            DelayModulation::Subtle => 2.0,
            DelayModulation::Classic => 5.0,
            DelayModulation::Heavy => 15.0,
        }
    }

    /// Get modulation rate in Hz
    fn rate(self) -> f32 {
        match self {
            DelayModulation::Off => 0.0,
            DelayModulation::Subtle => 0.3,
            DelayModulation::Classic => 0.8,
            DelayModulation::Heavy => 1.5,
        }
    }
}

/// High-quality stereo delay effect
pub struct Delay {
    sample_rate: f32,
    /// Delay buffer (stereo interleaved: L,R,L,R,...)
    buffer: Vec<f32>,
    /// Buffer length in stereo frames
    buffer_frames: usize,
    /// Write position (in frames, not samples)
    write_pos: usize,
    /// Delay time in fractional samples
    delay_samples: f32,
    /// Target delay (for smoothing)
    target_delay: f32,
    /// Delay smoothing coefficient
    delay_smooth: f32,
    /// Feedback amount (0.0 - 0.98)
    feedback: f32,
    /// Wet/dry mix (0.0 = dry, 1.0 = wet)
    mix: f32,
    /// Interpolation mode
    interpolation: DelayInterpolation,
    /// Modulation mode
    modulation: DelayModulation,
    /// Modulation LFO phase
    mod_phase: f32,
    /// Modulation LFO phase increment
    mod_phase_inc: f32,
    /// Highpass filter state for feedback (prevents mud)
    hp_state_l: f32,
    hp_state_r: f32,
    /// Highpass coefficient
    hp_coeff: f32,
    /// Enabled state
    enabled: bool,
    /// Wet envelope for click-free enable/disable
    wet_target: f32,
    wet_current: f32,
}

impl Delay {
    /// Create a new delay effect
    pub fn new(sample_rate: u32) -> Self {
        let sr = sample_rate as f32;
        let buffer_frames = (sr * MAX_DELAY_SECS) as usize;
        let buffer_size = buffer_frames * 2; // stereo

        // Highpass at 80Hz to prevent bass buildup
        let hp_coeff = (-2.0 * PI * 80.0 / sr).exp();

        Self {
            sample_rate: sr,
            buffer: vec![0.0; buffer_size],
            buffer_frames,
            write_pos: 0,
            delay_samples: sr / 2.0, // 500ms default
            target_delay: sr / 2.0,
            delay_smooth: 0.9995, // Very smooth to avoid clicks
            feedback: 0.3,
            mix: 0.5,
            interpolation: DelayInterpolation::Lagrange,
            modulation: DelayModulation::Off,
            mod_phase: 0.0,
            mod_phase_inc: 0.0,
            hp_state_l: 0.0,
            hp_state_r: 0.0,
            hp_coeff,
            enabled: false,
            wet_target: 0.0,
            wet_current: 0.0,
        }
    }

    /// Wet envelope smoothing coefficient (~10ms at 48kHz)
    const WET_SMOOTH_COEFF: f32 = 0.9995;

    /// Set delay time in milliseconds
    pub fn set_delay_ms(&mut self, ms: f32) {
        let max_ms = MAX_DELAY_SECS * 1000.0;
        let clamped_ms = ms.clamp(1.0, max_ms);
        self.target_delay = (clamped_ms / 1000.0) * self.sample_rate;
    }

    /// Get delay time in milliseconds
    pub fn delay_ms(&self) -> f32 {
        (self.delay_samples / self.sample_rate) * 1000.0
    }

    /// Set delay time synced to BPM
    pub fn set_delay_bpm_sync(&mut self, bpm: f32, beats: f32) {
        let beat_ms = 60000.0 / bpm.max(20.0);
        self.set_delay_ms(beat_ms * beats);
    }

    /// Set feedback amount (0.0 - 0.98)
    pub fn set_feedback(&mut self, feedback: f32) {
        self.feedback = feedback.clamp(0.0, 0.98);
    }

    /// Get feedback amount
    pub fn feedback(&self) -> f32 {
        self.feedback
    }

    /// Set wet/dry mix (0.0 - 1.0)
    pub fn set_mix(&mut self, mix: f32) {
        self.mix = mix.clamp(0.0, 1.0);
    }

    /// Get wet/dry mix
    pub fn mix(&self) -> f32 {
        self.mix
    }

    /// Set interpolation mode
    pub fn set_interpolation(&mut self, mode: DelayInterpolation) {
        self.interpolation = mode;
    }

    /// Set modulation mode
    pub fn set_modulation(&mut self, mode: DelayModulation) {
        self.modulation = mode;
        self.mod_phase_inc = mode.rate() / self.sample_rate;
    }

    /// Read from delay line with interpolation
    #[inline]
    fn read_interpolated(&self, delay_frames: f32) -> (f32, f32) {
        let int_delay = delay_frames as usize;
        let frac = delay_frames - int_delay as f32;

        // Calculate read position (circular buffer)
        let read_base = if self.write_pos >= int_delay {
            self.write_pos - int_delay
        } else {
            self.buffer_frames - (int_delay - self.write_pos)
        };

        match self.interpolation {
            DelayInterpolation::None => {
                let idx = read_base * 2;
                (self.buffer[idx], self.buffer[idx + 1])
            }
            DelayInterpolation::Linear => self.linear_interp(read_base, frac),
            DelayInterpolation::Lagrange => self.lagrange_interp(read_base, frac),
        }
    }

    /// Linear interpolation
    #[inline]
    fn linear_interp(&self, pos: usize, frac: f32) -> (f32, f32) {
        let idx0 = pos * 2;
        let pos1 = if pos == 0 {
            self.buffer_frames - 1
        } else {
            pos - 1
        };
        let idx1 = pos1 * 2;

        let l = self.buffer[idx0] * (1.0 - frac) + self.buffer[idx1] * frac;
        let r = self.buffer[idx0 + 1] * (1.0 - frac) + self.buffer[idx1 + 1] * frac;

        (l, r)
    }

    /// 4-point Lagrange cubic interpolation (highest quality)
    /// Uses polynomial through 4 neighboring samples
    #[inline]
    fn lagrange_interp(&self, pos: usize, frac: f32) -> (f32, f32) {
        // Get 4 sample positions: y[-1], y[0], y[1], y[2]
        let p0 = pos;
        let p_1 = if p0 + 1 >= self.buffer_frames {
            p0 + 1 - self.buffer_frames
        } else {
            p0 + 1
        };
        let p1 = if p0 == 0 {
            self.buffer_frames - 1
        } else {
            p0 - 1
        };
        let p2 = if p1 == 0 {
            self.buffer_frames - 1
        } else {
            p1 - 1
        };

        // Lagrange basis polynomials
        let x = frac;
        let x_1 = x + 1.0;
        let x_2 = x - 1.0;
        let x_3 = x - 2.0;

        // L_-1(x) = x(x-1)(x-2) / (-1)(-2)(-3) = -x(x-1)(x-2)/6
        let l_1 = -x * x_2 * x_3 / 6.0;
        // L_0(x) = (x+1)(x-1)(x-2) / (1)(-1)(-2) = (x+1)(x-1)(x-2)/2
        let l0 = x_1 * x_2 * x_3 / 2.0;
        // L_1(x) = (x+1)x(x-2) / (2)(1)(-1) = -(x+1)x(x-2)/2
        let l1 = -x_1 * x * x_3 / 2.0;
        // L_2(x) = (x+1)x(x-1) / (3)(2)(1) = (x+1)x(x-1)/6
        let l2 = x_1 * x * x_2 / 6.0;

        // Left channel
        let left = self.buffer[p_1 * 2] * l_1
            + self.buffer[p0 * 2] * l0
            + self.buffer[p1 * 2] * l1
            + self.buffer[p2 * 2] * l2;

        // Right channel
        let right = self.buffer[p_1 * 2 + 1] * l_1
            + self.buffer[p0 * 2 + 1] * l0
            + self.buffer[p1 * 2 + 1] * l1
            + self.buffer[p2 * 2 + 1] * l2;

        (left, right)
    }

    /// Soft saturation for feedback path
    #[inline(always)]
    fn soft_saturate(x: f32) -> f32 {
        // Fast tanh approximation
        x / (1.0 + x.abs())
    }

    /// Process modulation LFO
    #[inline]
    fn get_modulation(&mut self) -> f32 {
        if self.modulation == DelayModulation::Off {
            return 0.0;
        }

        // Sine LFO
        let mod_val = (self.mod_phase * 2.0 * PI).sin();
        self.mod_phase += self.mod_phase_inc;
        if self.mod_phase >= 1.0 {
            self.mod_phase -= 1.0;
        }

        mod_val * self.modulation.depth()
    }
}

impl Effect for Delay {
    fn process(&mut self, samples: &mut [f32]) {
        // Skip processing only if fully disabled and envelope has settled
        if !self.enabled && self.wet_current < 0.0001 {
            return;
        }

        for frame in samples.chunks_mut(2) {
            if frame.len() < 2 {
                continue;
            }

            // Smooth wet envelope toward target
            self.wet_current = Self::WET_SMOOTH_COEFF * self.wet_current
                + (1.0 - Self::WET_SMOOTH_COEFF) * self.wet_target;

            // Smooth delay time changes
            self.delay_samples = self.delay_samples * self.delay_smooth
                + self.target_delay * (1.0 - self.delay_smooth);

            // Apply modulation
            let mod_offset = self.get_modulation();
            let effective_delay =
                (self.delay_samples + mod_offset).clamp(4.0, self.buffer_frames as f32 - 4.0);

            // Read delayed signal with interpolation
            let (delayed_l, delayed_r) = self.read_interpolated(effective_delay);

            // Highpass filter on feedback path (prevent mud buildup)
            let hp_in_l = frame[0] + delayed_l * self.feedback;
            let hp_in_r = frame[1] + delayed_r * self.feedback;

            let hp_out_l = hp_in_l - self.hp_state_l;
            let hp_out_r = hp_in_r - self.hp_state_r;
            self.hp_state_l = hp_in_l * (1.0 - self.hp_coeff) + self.hp_state_l * self.hp_coeff;
            self.hp_state_r = hp_in_r * (1.0 - self.hp_coeff) + self.hp_state_r * self.hp_coeff;

            // Soft saturate feedback to prevent runaway
            let fb_l = Self::soft_saturate(hp_out_l);
            let fb_r = Self::soft_saturate(hp_out_r);

            // Write to buffer (only write input when enabled, allows tails to play out)
            let write_idx = self.write_pos * 2;
            if self.enabled {
                self.buffer[write_idx] = fb_l;
                self.buffer[write_idx + 1] = fb_r;
            } else {
                // When disabled, don't feed new input but let delay tails decay
                self.buffer[write_idx] = delayed_l * self.feedback * 0.95;
                self.buffer[write_idx + 1] = delayed_r * self.feedback * 0.95;
            }

            // Mix dry and wet signals with envelope
            let effective_mix = self.mix * self.wet_current;
            let dry = 1.0 - effective_mix;
            frame[0] = frame[0] * dry + delayed_l * effective_mix;
            frame[1] = frame[1] * dry + delayed_r * effective_mix;

            // Advance write position
            self.write_pos = (self.write_pos + 1) % self.buffer_frames;
        }
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.write_pos = 0;
        self.delay_samples = self.target_delay;
        self.hp_state_l = 0.0;
        self.hp_state_r = 0.0;
        self.mod_phase = 0.0;
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.wet_target = if enabled { 1.0 } else { 0.0 };
        // Note: don't reset on disable - let delay tails naturally fade out
    }

    fn name(&self) -> &'static str {
        "Delay"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delay_interpolation_modes() {
        for mode in [
            DelayInterpolation::None,
            DelayInterpolation::Linear,
            DelayInterpolation::Lagrange,
        ] {
            let mut delay = Delay::new(48000);
            delay.set_interpolation(mode);
            delay.set_enabled(true);
            delay.set_delay_ms(100.0);

            let mut samples = vec![1.0, 0.5, 0.0, -0.5, -1.0, -0.5, 0.0, 0.5];
            delay.process(&mut samples);

            // Should not panic or produce NaN
            assert!(samples.iter().all(|s| s.is_finite()));
        }
    }

    #[test]
    fn test_delay_bpm_sync() {
        let mut delay = Delay::new(48000);
        delay.set_delay_bpm_sync(120.0, 1.0); // 1 beat at 120 BPM = 500ms

        let expected_ms = 500.0;
        let actual_ms = delay.delay_ms();

        // Allow small floating point error due to smoothing
        assert!(
            (actual_ms - expected_ms).abs() < 1.0,
            "Expected ~{}ms, got {}ms",
            expected_ms,
            actual_ms
        );
    }

    #[test]
    fn test_modulation_modes() {
        for mode in [
            DelayModulation::Off,
            DelayModulation::Subtle,
            DelayModulation::Classic,
            DelayModulation::Heavy,
        ] {
            let mut delay = Delay::new(48000);
            delay.set_modulation(mode);
            delay.set_enabled(true);

            let mut samples = vec![0.5; 1024];
            delay.process(&mut samples);

            // Should not panic
            assert!(samples.iter().all(|s| s.is_finite()));
        }
    }

    #[test]
    fn test_soft_saturate() {
        // Should limit extreme values
        assert!(Delay::soft_saturate(10.0) < 1.0);
        assert!(Delay::soft_saturate(-10.0) > -1.0);
        // Near zero should be linear
        assert!((Delay::soft_saturate(0.1) - 0.091).abs() < 0.01);
    }
}
