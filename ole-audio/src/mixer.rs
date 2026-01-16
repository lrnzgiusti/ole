//! Mixer implementation - crossfader and channel routing

/// Crossfader curve type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CrossfaderCurve {
    /// Linear crossfade
    #[default]
    Linear,
    /// Constant power (equal loudness)
    ConstantPower,
    /// Sharp cut (DJ battle style)
    Cut,
}

/// Mixer for combining deck outputs
pub struct Mixer {
    /// Crossfader position (-1.0 = full A, 0.0 = center, 1.0 = full B)
    crossfader: f32,
    /// Crossfader curve
    curve: CrossfaderCurve,
    /// Master volume
    master_volume: f32,
}

impl Default for Mixer {
    fn default() -> Self {
        Self {
            crossfader: 0.0,
            curve: CrossfaderCurve::Linear,
            master_volume: 1.0,
        }
    }
}

impl Mixer {
    /// Create a new mixer
    pub fn new() -> Self {
        Self::default()
    }

    /// Set crossfader position (-1.0 to 1.0)
    pub fn set_crossfader(&mut self, position: f32) {
        self.crossfader = position.clamp(-1.0, 1.0);
    }

    /// Move crossfader by delta
    pub fn move_crossfader(&mut self, delta: f32) {
        self.set_crossfader(self.crossfader + delta);
    }

    /// Get crossfader position
    pub fn crossfader(&self) -> f32 {
        self.crossfader
    }

    /// Center the crossfader
    pub fn center_crossfader(&mut self) {
        self.crossfader = 0.0;
    }

    /// Set crossfader curve
    pub fn set_curve(&mut self, curve: CrossfaderCurve) {
        self.curve = curve;
    }

    /// Set master volume
    pub fn set_master_volume(&mut self, volume: f32) {
        self.master_volume = volume.clamp(0.0, 2.0);
    }

    /// Get master volume
    pub fn master_volume(&self) -> f32 {
        self.master_volume
    }

    /// Calculate gain for deck A based on crossfader
    fn gain_a(&self) -> f32 {
        match self.curve {
            CrossfaderCurve::Linear => {
                // -1.0 -> 1.0, 0.0 -> 0.5, 1.0 -> 0.0
                (1.0 - self.crossfader) * 0.5
            }
            CrossfaderCurve::ConstantPower => {
                // Constant power: use cosine curve
                let angle = (self.crossfader + 1.0) * std::f32::consts::FRAC_PI_4;
                angle.cos()
            }
            CrossfaderCurve::Cut => {
                // Sharp cut at edges
                if self.crossfader < 0.9 {
                    1.0
                } else {
                    (1.0 - self.crossfader) * 10.0
                }
            }
        }
    }

    /// Calculate gain for deck B based on crossfader
    fn gain_b(&self) -> f32 {
        match self.curve {
            CrossfaderCurve::Linear => (1.0 + self.crossfader) * 0.5,
            CrossfaderCurve::ConstantPower => {
                let angle = (self.crossfader + 1.0) * std::f32::consts::FRAC_PI_4;
                angle.sin()
            }
            CrossfaderCurve::Cut => {
                if self.crossfader > -0.9 {
                    1.0
                } else {
                    (1.0 + self.crossfader) * 10.0
                }
            }
        }
    }

    /// Mix two stereo buffers according to crossfader position
    /// Both inputs and output are interleaved stereo
    pub fn mix(&self, deck_a: &[f32], deck_b: &[f32], output: &mut [f32]) {
        let gain_a = self.gain_a() * self.master_volume;
        let gain_b = self.gain_b() * self.master_volume;

        let len = output.len().min(deck_a.len()).min(deck_b.len());

        for i in 0..len {
            output[i] = deck_a[i] * gain_a + deck_b[i] * gain_b;
        }

        // Soft clipping to prevent harsh distortion
        for sample in output.iter_mut() {
            *sample = soft_clip(*sample);
        }
    }
}

/// Soft clipper to prevent harsh digital clipping
#[inline(always)]
fn soft_clip(x: f32) -> f32 {
    if x.abs() < 0.9 {
        x
    } else {
        x.signum() * (0.9 + (1.0 - 0.9) * ((x.abs() - 0.9) / (1.0 - 0.9 + x.abs() - 0.9)))
    }
}
