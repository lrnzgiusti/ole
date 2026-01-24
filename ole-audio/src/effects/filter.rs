//! Biquad filter effect (high-pass, low-pass, band-pass)

use super::Effect;
use std::f32::consts::PI;

/// Filter type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FilterType {
    #[default]
    LowPass,
    HighPass,
    BandPass,
}

/// Biquad filter implementation
pub struct Filter {
    filter_type: FilterType,
    sample_rate: f32,
    cutoff: f32,    // Hz
    resonance: f32, // Q factor
    enabled: bool,

    // Biquad coefficients
    a0: f32,
    a1: f32,
    a2: f32,
    b1: f32,
    b2: f32,

    // State variables (stereo)
    x1_l: f32,
    x2_l: f32,
    y1_l: f32,
    y2_l: f32,
    x1_r: f32,
    x2_r: f32,
    y1_r: f32,
    y2_r: f32,

    // Wet envelope for click-free enable/disable
    wet_target: f32,
    wet_current: f32,
}

impl Filter {
    /// Wet envelope smoothing coefficient (~10ms at 48kHz)
    const WET_SMOOTH_COEFF: f32 = 0.9995;

    /// Create a new filter
    pub fn new(sample_rate: f32) -> Self {
        let mut filter = Self {
            filter_type: FilterType::LowPass,
            sample_rate,
            cutoff: 1000.0,
            resonance: 0.707, // Butterworth Q
            enabled: false,
            a0: 1.0,
            a1: 0.0,
            a2: 0.0,
            b1: 0.0,
            b2: 0.0,
            x1_l: 0.0,
            x2_l: 0.0,
            y1_l: 0.0,
            y2_l: 0.0,
            x1_r: 0.0,
            x2_r: 0.0,
            y1_r: 0.0,
            y2_r: 0.0,
            wet_target: 0.0,
            wet_current: 0.0,
        };
        filter.calculate_coefficients();
        filter
    }

    /// Set filter type
    pub fn set_type(&mut self, filter_type: FilterType) {
        self.filter_type = filter_type;
        self.calculate_coefficients();
    }

    /// Set cutoff frequency (20 - 20000 Hz)
    pub fn set_cutoff(&mut self, cutoff: f32) {
        self.cutoff = cutoff.clamp(20.0, 20000.0);
        self.calculate_coefficients();
    }

    /// Get cutoff frequency
    pub fn cutoff(&self) -> f32 {
        self.cutoff
    }

    /// Set resonance (0.1 - 20.0)
    pub fn set_resonance(&mut self, resonance: f32) {
        self.resonance = resonance.clamp(0.1, 20.0);
        self.calculate_coefficients();
    }

    /// Get resonance
    pub fn resonance(&self) -> f32 {
        self.resonance
    }

    /// Get filter type
    pub fn filter_type(&self) -> FilterType {
        self.filter_type
    }

    /// Calculate biquad coefficients based on current parameters
    fn calculate_coefficients(&mut self) {
        let omega = 2.0 * PI * self.cutoff / self.sample_rate;
        let sin_omega = omega.sin();
        let cos_omega = omega.cos();
        let alpha = sin_omega / (2.0 * self.resonance);

        match self.filter_type {
            FilterType::LowPass => {
                let b0 = (1.0 - cos_omega) / 2.0;
                let b1 = 1.0 - cos_omega;
                let b2 = (1.0 - cos_omega) / 2.0;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * cos_omega;
                let a2 = 1.0 - alpha;

                self.a0 = b0 / a0;
                self.a1 = b1 / a0;
                self.a2 = b2 / a0;
                self.b1 = a1 / a0;
                self.b2 = a2 / a0;
            }
            FilterType::HighPass => {
                let b0 = (1.0 + cos_omega) / 2.0;
                let b1 = -(1.0 + cos_omega);
                let b2 = (1.0 + cos_omega) / 2.0;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * cos_omega;
                let a2 = 1.0 - alpha;

                self.a0 = b0 / a0;
                self.a1 = b1 / a0;
                self.a2 = b2 / a0;
                self.b1 = a1 / a0;
                self.b2 = a2 / a0;
            }
            FilterType::BandPass => {
                let b0 = alpha;
                let b1 = 0.0;
                let b2 = -alpha;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * cos_omega;
                let a2 = 1.0 - alpha;

                self.a0 = b0 / a0;
                self.a1 = b1 / a0;
                self.a2 = b2 / a0;
                self.b1 = a1 / a0;
                self.b2 = a2 / a0;
            }
        }
    }

    /// Process a single sample (mono)
    fn process_sample(&mut self, input: f32, is_right: bool) -> f32 {
        let (x1, x2, y1, y2) = if is_right {
            (
                &mut self.x1_r,
                &mut self.x2_r,
                &mut self.y1_r,
                &mut self.y2_r,
            )
        } else {
            (
                &mut self.x1_l,
                &mut self.x2_l,
                &mut self.y1_l,
                &mut self.y2_l,
            )
        };

        let output =
            self.a0 * input + self.a1 * *x1 + self.a2 * *x2 - self.b1 * *y1 - self.b2 * *y2;

        *x2 = *x1;
        *x1 = input;
        *y2 = *y1;
        *y1 = output;

        output
    }
}

impl Effect for Filter {
    fn process(&mut self, samples: &mut [f32]) {
        // Skip processing only if fully disabled and envelope has settled
        if !self.enabled && self.wet_current < 0.0001 {
            return;
        }

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
        self.x1_l = 0.0;
        self.x2_l = 0.0;
        self.y1_l = 0.0;
        self.y2_l = 0.0;
        self.x1_r = 0.0;
        self.x2_r = 0.0;
        self.y1_r = 0.0;
        self.y2_r = 0.0;
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.wet_target = if enabled { 1.0 } else { 0.0 };
        // Note: don't reset filter state on disable - let it fade out naturally
    }

    fn name(&self) -> &'static str {
        match self.filter_type {
            FilterType::LowPass => "LP Filter",
            FilterType::HighPass => "HP Filter",
            FilterType::BandPass => "BP Filter",
        }
    }
}
