//! Turntable motor physics simulation
//!
//! Models:
//! - Motor startup curve (0 → target RPM over configurable time)
//! - Brake curve (target → 0 with configurable response)
//! - Momentum and inertia (smooth speed changes)
//! - Subtle pitch drift for analog imperfection

use std::f32::consts::PI;

/// Turntable motor physics simulation
pub struct TurntableMotor {
    sample_rate: f32,

    // Target and current speed (1.0 = normal playback)
    target_speed: f32,
    current_speed: f32,

    // Physics parameters
    startup_time: f32, // Seconds to reach full speed (0.5-2.0)
    brake_time: f32,   // Seconds to stop (0.3-1.0)

    // State
    is_running: bool,

    // Pitch drift (subtle analog imperfection)
    drift_phase: f32,
    drift_rate: f32,   // Hz (very slow, ~0.1)
    drift_amount: f32, // Fraction of pitch (0.001 = 0.1%)
}

impl TurntableMotor {
    /// Create a new turntable motor
    pub fn new(sample_rate: f32) -> Self {
        Self {
            sample_rate,
            target_speed: 1.0,
            current_speed: 1.0, // Start at full speed (already playing)
            startup_time: 0.8,
            brake_time: 0.5,
            is_running: true,
            drift_phase: 0.0,
            drift_rate: 0.08,    // Very slow drift
            drift_amount: 0.002, // 0.2% max pitch variation
        }
    }

    /// Set target playback speed (0.5-2.0)
    pub fn set_target_speed(&mut self, speed: f32) {
        self.target_speed = speed.clamp(0.0, 2.0);
    }

    /// Get target speed
    pub fn target_speed(&self) -> f32 {
        self.target_speed
    }

    /// Get current speed
    pub fn current_speed(&self) -> f32 {
        self.current_speed
    }

    /// Set startup time (seconds to reach full speed)
    pub fn set_startup_time(&mut self, seconds: f32) {
        self.startup_time = seconds.clamp(0.1, 3.0);
    }

    /// Set brake time (seconds to stop)
    pub fn set_brake_time(&mut self, seconds: f32) {
        self.brake_time = seconds.clamp(0.1, 2.0);
    }

    /// Start the motor (play)
    pub fn start(&mut self) {
        self.is_running = true;
    }

    /// Stop the motor (brake)
    pub fn stop(&mut self) {
        self.is_running = false;
    }

    /// Check if motor is running
    pub fn is_running(&self) -> bool {
        self.is_running
    }

    /// Reset to initial state
    pub fn reset(&mut self) {
        self.current_speed = 0.0;
        self.drift_phase = 0.0;
    }

    /// Get current playback rate modifier for one sample
    ///
    /// Call this once per sample in the deck's process loop.
    /// The returned value should be multiplied with the base tempo.
    #[inline]
    pub fn get_speed(&mut self) -> f32 {
        // Update motor speed based on whether we're starting or stopping
        if self.is_running {
            // Accelerate towards target
            if self.current_speed < self.target_speed {
                // Startup curve (smooth exponential approach)
                let rate = 1.0 / (self.startup_time * self.sample_rate);
                self.current_speed += (self.target_speed - self.current_speed) * rate * 60.0;
                self.current_speed = self.current_speed.min(self.target_speed);
            } else if self.current_speed > self.target_speed {
                // Slow down (tempo decrease)
                let rate = 1.0 / (self.brake_time * self.sample_rate);
                self.current_speed -= (self.current_speed - self.target_speed) * rate * 60.0;
                self.current_speed = self.current_speed.max(self.target_speed);
            }
        } else {
            // Braking - decelerate to zero
            if self.current_speed > 0.001 {
                let rate = 1.0 / (self.brake_time * self.sample_rate);
                self.current_speed *= 1.0 - rate * 120.0;
                if self.current_speed < 0.001 {
                    self.current_speed = 0.0;
                }
            }
        }

        // Apply inertia smoothing
        // (This is already handled by the exponential approach above)

        // Add subtle pitch drift (analog imperfection)
        self.drift_phase += self.drift_rate / self.sample_rate;
        if self.drift_phase >= 1.0 {
            self.drift_phase -= 1.0;
        }

        // Drift is a slow sine wave
        let drift = (self.drift_phase * 2.0 * PI).sin() * self.drift_amount;

        // Return speed with drift applied
        self.current_speed * (1.0 + drift)
    }

    /// Process a buffer of samples, returning speed for each sample
    /// This is more efficient than calling get_speed() for each sample
    pub fn process_buffer(&mut self, speeds: &mut [f32]) {
        for speed in speeds.iter_mut() {
            *speed = self.get_speed();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_motor_creation() {
        let motor = TurntableMotor::new(48000.0);
        assert!(motor.is_running());
        assert_eq!(motor.target_speed(), 1.0);
    }

    #[test]
    fn test_motor_stop_start() {
        let mut motor = TurntableMotor::new(48000.0);
        assert!(motor.is_running());

        motor.stop();
        assert!(!motor.is_running());

        motor.start();
        assert!(motor.is_running());
    }

    #[test]
    fn test_motor_braking() {
        let mut motor = TurntableMotor::new(48000.0);
        motor.stop();

        // Simulate braking over time
        for _ in 0..48000 {
            motor.get_speed();
        }

        // Should be nearly stopped after 1 second
        assert!(motor.current_speed() < 0.01);
    }

    #[test]
    fn test_speed_with_drift() {
        let mut motor = TurntableMotor::new(48000.0);
        let mut speeds = Vec::new();

        // Collect speeds over time
        for _ in 0..4800 {
            speeds.push(motor.get_speed());
        }

        // Speed should vary slightly due to drift
        let min = speeds.iter().cloned().fold(f32::INFINITY, f32::min);
        let max = speeds.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

        // Drift should cause small variation
        assert!(max - min < 0.01); // Less than 1% variation
        assert!(max - min > 0.0001); // But some variation
    }
}
