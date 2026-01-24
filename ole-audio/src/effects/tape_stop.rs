//! Tape Stop effect - simulates turntable/tape machine stopping
//!
//! Creates the dramatic slowdown effect used in DJ drops and transitions.
//! The pitch drops exponentially while the audio slows to a stop.

use super::Effect;

/// Tape stop effect with configurable stop time
pub struct TapeStop {
    enabled: bool,
    sample_rate: f32,

    /// Stop duration in seconds (0.1 - 5.0)
    stop_time: f32,

    /// Current playback rate (1.0 = normal, 0.0 = stopped)
    current_rate: f32,

    /// Target rate (0.0 when stopping, 1.0 when starting)
    target_rate: f32,

    /// Rate of rate change (exponential decay coefficient)
    rate_coefficient: f32,

    /// Trigger state - true when effect is actively stopping/starting
    triggered: bool,

    /// Direction: true = stopping, false = starting (spin-up)
    stopping: bool,

    /// Resampling state for pitch shifting
    resample_phase: f64,

    /// Previous samples for interpolation (stereo)
    prev_l: [f32; 4],
    prev_r: [f32; 4],

    /// Input buffer for resampling
    input_buffer: Vec<f32>,
    buffer_write_pos: usize,
    buffer_read_pos: f64,

    /// Wet envelope for click-free enable/disable
    wet_target: f32,
    wet_current: f32,
}

impl TapeStop {
    /// Wet envelope smoothing coefficient
    const WET_SMOOTH_COEFF: f32 = 0.9995;

    /// Buffer size for resampling (must be power of 2)
    const BUFFER_SIZE: usize = 8192;

    /// Create a new tape stop effect
    pub fn new(sample_rate: f32) -> Self {
        Self {
            enabled: false,
            sample_rate,
            stop_time: 1.0,
            current_rate: 1.0,
            target_rate: 1.0,
            rate_coefficient: 0.0,
            triggered: false,
            stopping: true,
            resample_phase: 0.0,
            prev_l: [0.0; 4],
            prev_r: [0.0; 4],
            input_buffer: vec![0.0; Self::BUFFER_SIZE * 2], // stereo
            buffer_write_pos: 0,
            buffer_read_pos: 0.0,
            wet_target: 0.0,
            wet_current: 0.0,
        }
    }

    /// Set stop time in seconds (0.1 - 5.0)
    pub fn set_stop_time(&mut self, seconds: f32) {
        self.stop_time = seconds.clamp(0.1, 5.0);
        self.update_coefficient();
    }

    /// Get stop time
    pub fn stop_time(&self) -> f32 {
        self.stop_time
    }

    /// Trigger the tape stop effect
    pub fn trigger_stop(&mut self) {
        self.triggered = true;
        self.stopping = true;
        self.target_rate = 0.0;
        self.update_coefficient();
    }

    /// Trigger spin-up (reverse of stop)
    pub fn trigger_start(&mut self) {
        self.triggered = true;
        self.stopping = false;
        self.target_rate = 1.0;
        self.update_coefficient();
    }

    /// Check if currently stopping
    pub fn is_stopping(&self) -> bool {
        self.triggered && self.stopping
    }

    /// Check if effect is complete (fully stopped or fully started)
    pub fn is_complete(&self) -> bool {
        !self.triggered
            || (self.stopping && self.current_rate < 0.001)
            || (!self.stopping && (self.current_rate - 1.0).abs() < 0.001)
    }

    /// Update the rate coefficient based on stop time
    fn update_coefficient(&mut self) {
        // Calculate coefficient for exponential decay over stop_time seconds
        // We want current_rate to reach ~0.001 after stop_time seconds
        let samples_for_stop = self.stop_time * self.sample_rate;
        self.rate_coefficient = (-7.0 / samples_for_stop).exp(); // e^(-7) ≈ 0.001
    }

    /// Cubic interpolation for smooth resampling
    #[inline]
    fn cubic_interpolate(y0: f32, y1: f32, y2: f32, y3: f32, t: f32) -> f32 {
        let a0 = y3 - y2 - y0 + y1;
        let a1 = y0 - y1 - a0;
        let a2 = y2 - y0;
        let a3 = y1;
        ((a0 * t + a1) * t + a2) * t + a3
    }

    /// Read from circular buffer with interpolation
    fn read_interpolated(&self, pos: f64, is_right: bool) -> f32 {
        let buffer_mask = Self::BUFFER_SIZE - 1;
        let channel_offset = if is_right { 1 } else { 0 };

        let pos_int = pos as usize;
        let frac = pos.fract() as f32;

        // Get 4 samples for cubic interpolation
        let i0 = ((pos_int.wrapping_sub(1)) & buffer_mask) * 2 + channel_offset;
        let i1 = (pos_int & buffer_mask) * 2 + channel_offset;
        let i2 = ((pos_int + 1) & buffer_mask) * 2 + channel_offset;
        let i3 = ((pos_int + 2) & buffer_mask) * 2 + channel_offset;

        Self::cubic_interpolate(
            self.input_buffer[i0],
            self.input_buffer[i1],
            self.input_buffer[i2],
            self.input_buffer[i3],
            frac,
        )
    }
}

impl Effect for TapeStop {
    fn process(&mut self, samples: &mut [f32]) {
        // Skip if not enabled and envelope settled
        if !self.enabled && self.wet_current < 0.0001 && !self.triggered {
            return;
        }

        let buffer_mask = Self::BUFFER_SIZE - 1;

        for frame in samples.chunks_mut(2) {
            if frame.len() < 2 {
                continue;
            }

            // Smooth wet envelope
            self.wet_current = Self::WET_SMOOTH_COEFF * self.wet_current
                + (1.0 - Self::WET_SMOOTH_COEFF) * self.wet_target;

            // Store input in circular buffer
            let write_idx = self.buffer_write_pos * 2;
            self.input_buffer[write_idx] = frame[0];
            self.input_buffer[write_idx + 1] = frame[1];
            self.buffer_write_pos = (self.buffer_write_pos + 1) & buffer_mask;

            // Update rate if triggered
            if self.triggered {
                if self.stopping {
                    self.current_rate *= self.rate_coefficient;
                    if self.current_rate < 0.001 {
                        self.current_rate = 0.0;
                        self.triggered = false;
                    }
                } else {
                    // Spin up - inverse exponential
                    self.current_rate = 1.0 - (1.0 - self.current_rate) * self.rate_coefficient;
                    if self.current_rate > 0.999 {
                        self.current_rate = 1.0;
                        self.triggered = false;
                    }
                }
            }

            // Read from buffer at variable rate
            let out_l = self.read_interpolated(self.buffer_read_pos, false);
            let out_r = self.read_interpolated(self.buffer_read_pos, true);

            // Advance read position by current rate
            self.buffer_read_pos += self.current_rate as f64;

            // Keep read position within buffer bounds (with some margin for interpolation)
            let max_pos = self.buffer_write_pos as f64 - 4.0;
            if self.buffer_read_pos > max_pos {
                self.buffer_read_pos = max_pos.max(0.0);
            }

            // Wrap read position
            while self.buffer_read_pos >= Self::BUFFER_SIZE as f64 {
                self.buffer_read_pos -= Self::BUFFER_SIZE as f64;
            }

            // Mix dry/wet with envelope
            let wet = self.wet_current;
            frame[0] = frame[0] * (1.0 - wet) + out_l * wet;
            frame[1] = frame[1] * (1.0 - wet) + out_r * wet;
        }
    }

    fn reset(&mut self) {
        self.current_rate = 1.0;
        self.target_rate = 1.0;
        self.triggered = false;
        self.resample_phase = 0.0;
        self.prev_l = [0.0; 4];
        self.prev_r = [0.0; 4];
        self.input_buffer.fill(0.0);
        self.buffer_write_pos = 0;
        self.buffer_read_pos = 0.0;
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.wet_target = if enabled { 1.0 } else { 0.0 };
        if enabled {
            // Reset rate to normal when enabling
            self.current_rate = 1.0;
            self.triggered = false;
        }
    }

    fn name(&self) -> &'static str {
        "Tape Stop"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tape_stop_creation() {
        let ts = TapeStop::new(48000.0);
        assert!(!ts.is_enabled());
        assert_eq!(ts.stop_time(), 1.0);
    }

    #[test]
    fn test_tape_stop_trigger() {
        let mut ts = TapeStop::new(48000.0);
        ts.set_enabled(true);
        ts.trigger_stop();
        assert!(ts.is_stopping());
    }
}
