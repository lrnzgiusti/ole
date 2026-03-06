//! Rhythmic gate effect - BPM-synced volume gate
//!
//! Classic DJ effect that chops audio into rhythmic patterns by
//! applying a BPM-synced volume envelope. Supports multiple note
//! divisions, duty cycle control, and gate shapes.

use super::Effect;

/// Note division for the gate rhythm
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GateDivision {
    Half,
    Quarter,
    #[default]
    Eighth,
    Sixteenth,
    ThirtySecond,
    Triplet8,
    Triplet16,
}

impl GateDivision {
    /// Returns the length of this division in beats
    pub fn beats(self) -> f32 {
        match self {
            Self::Half => 2.0,
            Self::Quarter => 1.0,
            Self::Eighth => 0.5,
            Self::Sixteenth => 0.25,
            Self::ThirtySecond => 0.125,
            Self::Triplet8 => 1.0 / 3.0,
            Self::Triplet16 => 1.0 / 6.0,
        }
    }

    /// Cycle to the next division
    pub fn next(self) -> Self {
        match self {
            Self::Half => Self::Quarter,
            Self::Quarter => Self::Eighth,
            Self::Eighth => Self::Sixteenth,
            Self::Sixteenth => Self::ThirtySecond,
            Self::ThirtySecond => Self::Triplet8,
            Self::Triplet8 => Self::Triplet16,
            Self::Triplet16 => Self::Half,
        }
    }
}

/// Shape of the gate envelope
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GateShape {
    /// Hard on/off gating
    #[default]
    Square,
    /// Smooth attack/release (~2ms)
    Soft,
    /// Decaying ramp during open phase
    Ramp,
}

/// Rhythmic gate effect with BPM sync
pub struct Gate {
    enabled: bool,
    sample_rate: f32,

    /// Note division (default: Eighth)
    division: GateDivision,

    /// Duty cycle - fraction of period the gate is open (0.1 - 0.9)
    duty_cycle: f32,

    /// Gate envelope shape
    shape: GateShape,

    /// Wet/dry mix (0.0 - 1.0)
    mix: f32,

    /// Tempo in BPM
    bpm: f32,

    /// Gate phase (0.0 - 1.0, wrapping)
    phase: f32,

    /// Smoothed gate envelope value
    envelope: f32,

    /// Wet envelope for click-free enable/disable
    wet_target: f32,
    wet_current: f32,
}

impl Gate {
    /// Wet envelope smoothing coefficient
    const WET_SMOOTH_COEFF: f32 = 0.9995;

    /// Create a new gate effect
    pub fn new(sample_rate: f32) -> Self {
        Self {
            enabled: false,
            sample_rate,
            division: GateDivision::default(),
            duty_cycle: 0.5,
            shape: GateShape::default(),
            mix: 1.0,
            bpm: 120.0,
            phase: 0.0,
            envelope: 0.0,
            wet_target: 0.0,
            wet_current: 0.0,
        }
    }

    /// Set note division
    pub fn set_division(&mut self, division: GateDivision) {
        self.division = division;
    }

    /// Get note division
    pub fn division(&self) -> GateDivision {
        self.division
    }

    /// Set duty cycle (0.1 - 0.9)
    pub fn set_duty_cycle(&mut self, duty_cycle: f32) {
        self.duty_cycle = duty_cycle.clamp(0.1, 0.9);
    }

    /// Get duty cycle
    pub fn duty_cycle(&self) -> f32 {
        self.duty_cycle
    }

    /// Set gate shape
    pub fn set_shape(&mut self, shape: GateShape) {
        self.shape = shape;
    }

    /// Get gate shape
    pub fn shape(&self) -> GateShape {
        self.shape
    }

    /// Set wet/dry mix (0.0 - 1.0)
    pub fn set_mix(&mut self, mix: f32) {
        self.mix = mix.clamp(0.0, 1.0);
    }

    /// Get mix
    pub fn mix(&self) -> f32 {
        self.mix
    }

    /// Set tempo in BPM
    pub fn set_bpm(&mut self, bpm: f32) {
        if !bpm.is_finite() {
            return;
        }
        self.bpm = bpm.clamp(20.0, 300.0);
    }

    /// Get BPM
    pub fn bpm(&self) -> f32 {
        self.bpm
    }

    /// Calculate the smoothing coefficient for a given time in seconds
    #[inline]
    fn smooth_coeff(time_secs: f32, sample_rate: f32) -> f32 {
        (-1.0 / (time_secs * sample_rate)).exp()
    }
}

impl Effect for Gate {
    fn process(&mut self, samples: &mut [f32]) {
        // Skip if fully disabled and envelope settled
        if !self.enabled && self.wet_current < 0.0001 {
            return;
        }

        // Phase increment per sample
        // Period = (60/bpm) * beats_per_division seconds
        let samples_per_div = (self.sample_rate * 60.0 * self.division.beats()) / self.bpm;
        let phase_inc = 1.0 / samples_per_div;

        // Envelope smoothing coefficient depends on shape:
        // - Soft: ~2ms for musical attack/release
        // - Square/Ramp: ~0.2ms just to prevent clicks
        let env_coeff = match self.shape {
            GateShape::Soft => Self::smooth_coeff(0.002, self.sample_rate),
            _ => Self::smooth_coeff(0.0002, self.sample_rate),
        };

        for frame in samples.chunks_mut(2) {
            if frame.len() < 2 {
                continue;
            }

            // Smooth wet envelope
            self.wet_current = Self::WET_SMOOTH_COEFF * self.wet_current
                + (1.0 - Self::WET_SMOOTH_COEFF) * self.wet_target;

            // Calculate gate target based on shape
            let target = match self.shape {
                GateShape::Square => {
                    if self.phase < self.duty_cycle {
                        1.0
                    } else {
                        0.0
                    }
                }
                GateShape::Soft => {
                    if self.phase < self.duty_cycle {
                        1.0
                    } else {
                        0.0
                    }
                }
                GateShape::Ramp => {
                    if self.phase < self.duty_cycle {
                        1.0 - self.phase / self.duty_cycle
                    } else {
                        0.0
                    }
                }
            };

            // Smooth envelope to prevent clicks
            self.envelope = env_coeff * self.envelope + (1.0 - env_coeff) * target;

            // Apply gate with wet/dry mix
            let effective_mix = self.mix * self.wet_current;
            let dry_l = frame[0];
            let dry_r = frame[1];
            frame[0] = dry_l * (1.0 - effective_mix) + dry_l * self.envelope * effective_mix;
            frame[1] = dry_r * (1.0 - effective_mix) + dry_r * self.envelope * effective_mix;

            // Advance phase
            self.phase += phase_inc;
            if self.phase >= 1.0 {
                self.phase -= 1.0;
            }
        }
    }

    fn reset(&mut self) {
        self.phase = 0.0;
        self.envelope = 0.0;
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.wet_target = if enabled { 1.0 } else { 0.0 };
    }

    fn name(&self) -> &'static str {
        "Gate"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gate_creation() {
        let gate = Gate::new(48000.0);
        assert!(!gate.is_enabled());
        assert_eq!(gate.bpm(), 120.0);
        assert_eq!(gate.division(), GateDivision::Eighth);
        assert_eq!(gate.duty_cycle(), 0.5);
        assert_eq!(gate.shape(), GateShape::Square);
        assert_eq!(gate.mix(), 1.0);
        assert_eq!(gate.name(), "Gate");
    }

    #[test]
    fn test_gate_parameter_clamping() {
        let mut gate = Gate::new(48000.0);

        gate.set_duty_cycle(0.0);
        assert_eq!(gate.duty_cycle(), 0.1);

        gate.set_duty_cycle(1.0);
        assert_eq!(gate.duty_cycle(), 0.9);

        gate.set_mix(-0.5);
        assert_eq!(gate.mix(), 0.0);

        gate.set_mix(1.5);
        assert_eq!(gate.mix(), 1.0);

        gate.set_bpm(5.0);
        assert_eq!(gate.bpm(), 20.0);

        gate.set_bpm(500.0);
        assert_eq!(gate.bpm(), 300.0);
    }

    #[test]
    fn test_gate_division_beats() {
        assert_eq!(GateDivision::Half.beats(), 2.0);
        assert_eq!(GateDivision::Quarter.beats(), 1.0);
        assert_eq!(GateDivision::Eighth.beats(), 0.5);
        assert_eq!(GateDivision::Sixteenth.beats(), 0.25);
        assert_eq!(GateDivision::ThirtySecond.beats(), 0.125);
    }

    #[test]
    fn test_gate_division_cycle() {
        let div = GateDivision::Half;
        let div = div.next(); // Quarter
        assert_eq!(div, GateDivision::Quarter);
        let div = div.next(); // Eighth
        assert_eq!(div, GateDivision::Eighth);
        let div = div.next(); // Sixteenth
        assert_eq!(div, GateDivision::Sixteenth);
        let div = div.next(); // ThirtySecond
        assert_eq!(div, GateDivision::ThirtySecond);
        let div = div.next(); // Triplet8
        assert_eq!(div, GateDivision::Triplet8);
        let div = div.next(); // Triplet16
        assert_eq!(div, GateDivision::Triplet16);
        let div = div.next(); // wraps to Half
        assert_eq!(div, GateDivision::Half);
    }

    #[test]
    fn test_gate_bypass_when_disabled() {
        let mut gate = Gate::new(48000.0);
        let mut samples = vec![0.5, 0.5, 0.3, 0.3, 0.1, 0.1];
        let original = samples.clone();
        gate.process(&mut samples);

        // Disabled with wet_current at 0 should pass through unchanged
        assert_eq!(samples, original);
    }

    #[test]
    fn test_gate_processes_audio() {
        let mut gate = Gate::new(48000.0);
        gate.set_enabled(true);
        gate.wet_current = 1.0; // Force wet for test

        // Generate enough samples to cover a full gate cycle
        // At 120 BPM, eighth note = 0.25s = 12000 samples at 48kHz
        let num_frames = 12000;
        let mut samples = vec![1.0; num_frames * 2]; // Stereo
        gate.process(&mut samples);

        // With square gate at 50% duty, roughly half the frames should be
        // near-silent (envelope close to 0) and half should be near-unity
        let mut near_zero = 0;
        let mut near_one = 0;
        for frame in samples.chunks(2) {
            if frame[0].abs() < 0.1 {
                near_zero += 1;
            } else if frame[0] > 0.9 {
                near_one += 1;
            }
        }

        // Both counts should be substantial (allowing for envelope transitions)
        assert!(near_zero > 4000, "Expected many gated frames, got {near_zero}");
        assert!(near_one > 4000, "Expected many open frames, got {near_one}");
    }

    #[test]
    fn test_gate_ramp_shape() {
        let mut gate = Gate::new(48000.0);
        gate.set_shape(GateShape::Ramp);
        gate.set_enabled(true);
        gate.wet_current = 1.0;
        gate.envelope = 1.0; // Pre-settled envelope

        let mut samples = vec![1.0; 2000];
        gate.process(&mut samples);

        // After a few samples for the envelope to settle, ramp should be high
        // then decay towards 0 during the open phase
        assert!(samples[20] > 0.8, "Ramp should start high near beginning");

        // Later in the open phase the ramp should have decayed
        let mid = samples.len() / 2;
        assert!(
            samples[mid] < samples[20],
            "Ramp should decay over the open phase"
        );
    }

    #[test]
    fn test_gate_reset() {
        let mut gate = Gate::new(48000.0);
        gate.set_enabled(true);
        gate.wet_current = 1.0;
        gate.phase = 0.7;
        gate.envelope = 0.5;

        gate.reset();

        assert_eq!(gate.phase, 0.0);
        assert_eq!(gate.envelope, 0.0);
    }
}
