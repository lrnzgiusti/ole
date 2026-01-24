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
    /// Smoothed crossfader position (interpolates toward crossfader)
    smoothed_crossfader: f32,
    /// Crossfader curve
    curve: CrossfaderCurve,
    /// Master volume
    master_volume: f32,
    /// Smoothed master volume (interpolates toward master_volume to prevent clicks)
    smoothed_master_volume: f32,
}

impl Mixer {
    /// Smoothing coefficient for crossfader (~5ms at 48kHz)
    const CROSSFADER_SMOOTH_COEFF: f32 = 0.995;
    /// Smoothing coefficient for master volume (~5ms at 48kHz)
    const MASTER_VOLUME_SMOOTH_COEFF: f32 = 0.995;
}

impl Default for Mixer {
    fn default() -> Self {
        Self {
            crossfader: 0.0,
            smoothed_crossfader: 0.0,
            curve: CrossfaderCurve::Linear,
            master_volume: 1.0,
            smoothed_master_volume: 1.0,
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

    /// Calculate gain for deck A based on crossfader position
    fn gain_a_for(&self, cf: f32) -> f32 {
        match self.curve {
            CrossfaderCurve::Linear => {
                // -1.0 -> 1.0, 0.0 -> 0.5, 1.0 -> 0.0
                (1.0 - cf) * 0.5
            }
            CrossfaderCurve::ConstantPower => {
                // Constant power: use cosine curve
                let angle = (cf + 1.0) * std::f32::consts::FRAC_PI_4;
                angle.cos()
            }
            CrossfaderCurve::Cut => {
                // Sharp cut at edges
                if cf < 0.9 {
                    1.0
                } else {
                    (1.0 - cf) * 10.0
                }
            }
        }
    }

    /// Calculate gain for deck B based on crossfader position
    fn gain_b_for(&self, cf: f32) -> f32 {
        match self.curve {
            CrossfaderCurve::Linear => (1.0 + cf) * 0.5,
            CrossfaderCurve::ConstantPower => {
                let angle = (cf + 1.0) * std::f32::consts::FRAC_PI_4;
                angle.sin()
            }
            CrossfaderCurve::Cut => {
                if cf > -0.9 {
                    1.0
                } else {
                    (1.0 + cf) * 10.0
                }
            }
        }
    }

    /// Mix two stereo buffers according to crossfader position
    /// Both inputs and output are interleaved stereo
    /// Uses per-sample smoothing to prevent clicks during crossfader and volume changes
    pub fn mix(&mut self, deck_a: &[f32], deck_b: &[f32], output: &mut [f32]) {
        let len = output.len().min(deck_a.len()).min(deck_b.len());

        // Process in stereo frames (2 samples per frame)
        for i in (0..len).step_by(2) {
            // Smooth crossfader toward target position
            self.smoothed_crossfader = Self::CROSSFADER_SMOOTH_COEFF * self.smoothed_crossfader
                + (1.0 - Self::CROSSFADER_SMOOTH_COEFF) * self.crossfader;

            // Smooth master volume toward target position
            self.smoothed_master_volume = Self::MASTER_VOLUME_SMOOTH_COEFF
                * self.smoothed_master_volume
                + (1.0 - Self::MASTER_VOLUME_SMOOTH_COEFF) * self.master_volume;

            // Calculate gains from smoothed crossfader and smoothed master volume
            let gain_a = self.gain_a_for(self.smoothed_crossfader) * self.smoothed_master_volume;
            let gain_b = self.gain_b_for(self.smoothed_crossfader) * self.smoothed_master_volume;

            // Mix left channel
            output[i] = deck_a[i] * gain_a + deck_b[i] * gain_b;

            // Mix right channel (if present)
            if i + 1 < len {
                output[i + 1] = deck_a[i + 1] * gain_a + deck_b[i + 1] * gain_b;
            }
        }

        // Soft clipping to prevent harsh distortion
        for sample in output.iter_mut() {
            *sample = soft_clip(*sample);
        }
    }
}

/// Soft clip threshold - lower value gives limiter more time to react
const SOFT_CLIP_THRESHOLD: f32 = 0.75;
/// Soft clip ceiling - matches limiter ceiling minus margin
const SOFT_CLIP_CEILING: f32 = 0.89;

/// Gentle soft clipper for mix bus
///
/// Very transparent limiting - only activates on peaks above threshold.
/// Uses a smooth polynomial curve that preserves dynamics while
/// preventing harsh digital clipping. Threshold set below limiter ceiling
/// to give the limiter clean input signal.
#[inline(always)]
fn soft_clip(x: f32) -> f32 {
    let abs_x = x.abs();

    // Below threshold: pass through unchanged (fully transparent)
    if abs_x <= SOFT_CLIP_THRESHOLD {
        return x;
    }

    // Soft knee region: gentle compression
    // Uses exponential curve for smooth transition
    let sign = x.signum();
    let knee_width = SOFT_CLIP_CEILING - SOFT_CLIP_THRESHOLD;
    let over = abs_x - SOFT_CLIP_THRESHOLD;
    let ratio = over / knee_width; // How far into the knee (0.0 to 1.0+)

    // Asymptotic approach to ceiling with gentle curve
    let compressed = SOFT_CLIP_THRESHOLD + knee_width * (1.0 - (-ratio * 3.0).exp());
    sign * compressed.min(SOFT_CLIP_CEILING)
}
