//! State Variable Filter (SVF) - Clean digital filter
//!
//! Based on Andrew Simper's (Cytomic) SVF design using
//! trapezoidal integration for stability and accuracy.
//!
//! Features:
//! - All outputs available: LP, HP, BP, Notch
//! - Stable at high resonance (no self-oscillation)
//! - Clean digital character
//! - Parameter smoothing

use super::Effect;
use std::f32::consts::PI;

/// SVF output type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SvfOutputType {
    #[default]
    LowPass,
    HighPass,
    BandPass,
    Notch,
}

/// State Variable Filter
pub struct StateVariableFilter {
    enabled: bool,
    sample_rate: f32,

    // Parameters
    cutoff: f32,    // Hz (20-20000)
    resonance: f32, // 0.0-1.0 (Q from 0.5 to 20)
    output_type: SvfOutputType,

    // Filter state (stereo)
    ic1eq_l: f32,
    ic2eq_l: f32,
    ic1eq_r: f32,
    ic2eq_r: f32,

    // Coefficients (recalculated on parameter change)
    g: f32,
    k: f32,
    a1: f32,
    a2: f32,
    a3: f32,

    // Parameter smoothing
    cutoff_smooth: f32,
    resonance_smooth: f32,
    smoothing_coeff: f32,

    // Track if coefficients need update
    coeffs_dirty: bool,

    // Wet envelope for click-free enable/disable
    wet_target: f32,
    wet_current: f32,
}

impl StateVariableFilter {
    /// Create a new SVF
    pub fn new(sample_rate: f32) -> Self {
        let mut filter = Self {
            enabled: false,
            sample_rate,
            cutoff: 1000.0,
            resonance: 0.5,
            output_type: SvfOutputType::LowPass,
            ic1eq_l: 0.0,
            ic2eq_l: 0.0,
            ic1eq_r: 0.0,
            ic2eq_r: 0.0,
            g: 0.0,
            k: 0.0,
            a1: 0.0,
            a2: 0.0,
            a3: 0.0,
            cutoff_smooth: 1000.0,
            resonance_smooth: 0.5,
            smoothing_coeff: 1.0 - (-1.0 / (0.005 * sample_rate)).exp(),
            coeffs_dirty: true,
            wet_target: 0.0,
            wet_current: 0.0,
        };
        filter.calculate_coefficients();
        filter
    }

    /// Wet envelope smoothing coefficient (~10ms at 48kHz)
    const WET_SMOOTH_COEFF: f32 = 0.9995;

    /// Set cutoff frequency (20-20000 Hz)
    pub fn set_cutoff(&mut self, cutoff: f32) {
        let new_cutoff = cutoff.clamp(20.0, 20000.0);
        if (new_cutoff - self.cutoff).abs() > 0.01 {
            self.cutoff = new_cutoff;
            self.coeffs_dirty = true;
        }
    }

    /// Get cutoff frequency
    pub fn cutoff(&self) -> f32 {
        self.cutoff
    }

    /// Set resonance (0.0-1.0)
    pub fn set_resonance(&mut self, resonance: f32) {
        let new_res = resonance.clamp(0.0, 1.0);
        if (new_res - self.resonance).abs() > 0.001 {
            self.resonance = new_res;
            self.coeffs_dirty = true;
        }
    }

    /// Get resonance
    pub fn resonance(&self) -> f32 {
        self.resonance
    }

    /// Set output type
    pub fn set_output_type(&mut self, output_type: SvfOutputType) {
        self.output_type = output_type;
    }

    /// Get output type
    pub fn output_type(&self) -> SvfOutputType {
        self.output_type
    }

    /// Calculate coefficients using Cytomic's formulas
    fn calculate_coefficients(&mut self) {
        // Prewarp the cutoff frequency
        let g_raw = (PI * self.cutoff_smooth / self.sample_rate).tan();

        // Convert resonance (0-1) to Q (0.5-20) then to k
        // k = 1/Q, where Q ranges from 0.5 (wide) to 20 (narrow)
        let q = 0.5 + self.resonance_smooth * 19.5;
        let k = 1.0 / q;

        // SVF coefficients
        self.g = g_raw;
        self.k = k;
        self.a1 = 1.0 / (1.0 + g_raw * (g_raw + k));
        self.a2 = g_raw * self.a1;
        self.a3 = g_raw * self.a2;

        self.coeffs_dirty = false;
    }

    /// Process a single sample and return the selected output
    #[inline]
    fn process_sample(&mut self, input: f32, is_right: bool) -> f32 {
        // Select channel state
        let (ic1eq, ic2eq) = if is_right {
            (&mut self.ic1eq_r, &mut self.ic2eq_r)
        } else {
            (&mut self.ic1eq_l, &mut self.ic2eq_l)
        };

        // SVF tick (trapezoidal integration)
        let v3 = input - *ic2eq;
        let v1 = self.a1 * *ic1eq + self.a2 * v3;
        let v2 = *ic2eq + self.a2 * *ic1eq + self.a3 * v3;

        // Update state
        *ic1eq = 2.0 * v1 - *ic1eq;
        *ic2eq = 2.0 * v2 - *ic2eq;

        // Calculate all outputs
        let low = v2;
        let band = v1;
        let high = input - self.k * band - low;
        let notch = low + high;

        // Return selected output
        match self.output_type {
            SvfOutputType::LowPass => low,
            SvfOutputType::HighPass => high,
            SvfOutputType::BandPass => band,
            SvfOutputType::Notch => notch,
        }
    }

    /// Smooth parameters and update coefficients if needed
    fn update_params(&mut self) {
        let old_cutoff = self.cutoff_smooth;
        let old_res = self.resonance_smooth;

        self.cutoff_smooth += (self.cutoff - self.cutoff_smooth) * self.smoothing_coeff;
        self.resonance_smooth += (self.resonance - self.resonance_smooth) * self.smoothing_coeff;

        // Recalculate if smoothed values changed significantly
        if self.coeffs_dirty
            || (self.cutoff_smooth - old_cutoff).abs() > 0.1
            || (self.resonance_smooth - old_res).abs() > 0.001
        {
            self.calculate_coefficients();
        }
    }
}

impl Effect for StateVariableFilter {
    fn process(&mut self, samples: &mut [f32]) {
        // Skip processing only if fully disabled and envelope has settled
        if !self.enabled && self.wet_current < 0.0001 {
            return;
        }

        // Update parameters at start of buffer
        self.update_params();

        for frame in samples.chunks_mut(2) {
            if frame.len() == 2 {
                // Smooth wet envelope toward target
                self.wet_current = Self::WET_SMOOTH_COEFF * self.wet_current
                    + (1.0 - Self::WET_SMOOTH_COEFF) * self.wet_target;

                // Process through filter
                let wet_l = self.process_sample(frame[0], false);
                let wet_r = self.process_sample(frame[1], true);

                // Crossfade between dry and wet based on envelope
                frame[0] = frame[0] * (1.0 - self.wet_current) + wet_l * self.wet_current;
                frame[1] = frame[1] * (1.0 - self.wet_current) + wet_r * self.wet_current;
            }
        }
    }

    fn reset(&mut self) {
        self.ic1eq_l = 0.0;
        self.ic2eq_l = 0.0;
        self.ic1eq_r = 0.0;
        self.ic2eq_r = 0.0;
        self.cutoff_smooth = self.cutoff;
        self.resonance_smooth = self.resonance;
        self.coeffs_dirty = true;
        self.calculate_coefficients();
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.wet_target = if enabled { 1.0 } else { 0.0 };
        // Note: don't reset on disable - let filter state fade out naturally
    }

    fn name(&self) -> &'static str {
        match self.output_type {
            SvfOutputType::LowPass => "SVF LP",
            SvfOutputType::HighPass => "SVF HP",
            SvfOutputType::BandPass => "SVF BP",
            SvfOutputType::Notch => "SVF Notch",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_svf_creation() {
        let filter = StateVariableFilter::new(48000.0);
        assert!(!filter.is_enabled());
        assert_eq!(filter.cutoff(), 1000.0);
        assert_eq!(filter.output_type(), SvfOutputType::LowPass);
    }

    #[test]
    fn test_svf_parameter_clamping() {
        let mut filter = StateVariableFilter::new(48000.0);

        filter.set_cutoff(10.0);
        assert_eq!(filter.cutoff(), 20.0);

        filter.set_cutoff(30000.0);
        assert_eq!(filter.cutoff(), 20000.0);

        filter.set_resonance(-0.5);
        assert_eq!(filter.resonance(), 0.0);

        filter.set_resonance(1.5);
        assert_eq!(filter.resonance(), 1.0);
    }

    #[test]
    fn test_svf_output_types() {
        let mut filter = StateVariableFilter::new(48000.0);

        filter.set_output_type(SvfOutputType::HighPass);
        assert_eq!(filter.output_type(), SvfOutputType::HighPass);
        assert_eq!(filter.name(), "SVF HP");

        filter.set_output_type(SvfOutputType::BandPass);
        assert_eq!(filter.output_type(), SvfOutputType::BandPass);
        assert_eq!(filter.name(), "SVF BP");

        filter.set_output_type(SvfOutputType::Notch);
        assert_eq!(filter.output_type(), SvfOutputType::Notch);
        assert_eq!(filter.name(), "SVF Notch");
    }

    #[test]
    fn test_svf_processes_audio() {
        let mut filter = StateVariableFilter::new(48000.0);
        filter.set_enabled(true);
        filter.set_cutoff(1000.0);
        filter.set_resonance(0.5);

        let mut samples = vec![0.5, 0.5, 0.3, 0.3, 0.1, 0.1, -0.1, -0.1];
        filter.process(&mut samples);

        // Output should be modified
        assert_ne!(samples[0], 0.5);
    }
}
