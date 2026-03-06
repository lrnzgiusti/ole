//! 3-Band Channel EQ (Low/Mid/High) with kill switches
//! ISO-style EQ with complete frequency isolation

use super::Effect;

/// Biquad filter state for one channel
#[derive(Debug, Clone, Copy, Default)]
struct BiquadState {
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl BiquadState {
    #[inline(always)]
    fn process(&mut self, input: f32, b0: f32, b1: f32, b2: f32, a1: f32, a2: f32) -> f32 {
        let output = b0 * input + b1 * self.x1 + b2 * self.x2 - a1 * self.y1 - a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = input;
        self.y2 = self.y1;
        self.y1 = output;
        output
    }

    fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Coefficients for a biquad filter
#[derive(Debug, Clone, Copy)]
struct BiquadCoeffs {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl Default for BiquadCoeffs {
    fn default() -> Self {
        Self { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 }
    }
}

impl BiquadCoeffs {
    /// Low shelf filter
    fn low_shelf(freq: f32, gain_db: f32, sample_rate: f32) -> Self {
        let a = 10.0_f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * freq / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / 2.0 * ((a + 1.0 / a) * (1.0 / 0.707 - 1.0) + 2.0).sqrt();

        let a0 = (a + 1.0) + (a - 1.0) * cos_w0 + 2.0 * alpha * a.sqrt();
        let a0_inv = 1.0 / a0;

        Self {
            b0: (a * ((a + 1.0) - (a - 1.0) * cos_w0 + 2.0 * alpha * a.sqrt())) * a0_inv,
            b1: (2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0)) * a0_inv,
            b2: (a * ((a + 1.0) - (a - 1.0) * cos_w0 - 2.0 * alpha * a.sqrt())) * a0_inv,
            a1: (-2.0 * ((a - 1.0) + (a + 1.0) * cos_w0)) * a0_inv,
            a2: ((a + 1.0) + (a - 1.0) * cos_w0 - 2.0 * alpha * a.sqrt()) * a0_inv,
        }
    }

    /// High shelf filter
    fn high_shelf(freq: f32, gain_db: f32, sample_rate: f32) -> Self {
        let a = 10.0_f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * freq / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / 2.0 * ((a + 1.0 / a) * (1.0 / 0.707 - 1.0) + 2.0).sqrt();

        let a0 = (a + 1.0) - (a - 1.0) * cos_w0 + 2.0 * alpha * a.sqrt();
        let a0_inv = 1.0 / a0;

        Self {
            b0: (a * ((a + 1.0) + (a - 1.0) * cos_w0 + 2.0 * alpha * a.sqrt())) * a0_inv,
            b1: (-2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0)) * a0_inv,
            b2: (a * ((a + 1.0) + (a - 1.0) * cos_w0 - 2.0 * alpha * a.sqrt())) * a0_inv,
            a1: (2.0 * ((a - 1.0) - (a + 1.0) * cos_w0)) * a0_inv,
            a2: ((a + 1.0) - (a - 1.0) * cos_w0 - 2.0 * alpha * a.sqrt()) * a0_inv,
        }
    }

    /// Peaking EQ filter
    fn peaking(freq: f32, gain_db: f32, q: f32, sample_rate: f32) -> Self {
        let a = 10.0_f32.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f32::consts::PI * freq / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);

        let a0 = 1.0 + alpha / a;
        let a0_inv = 1.0 / a0;

        Self {
            b0: (1.0 + alpha * a) * a0_inv,
            b1: (-2.0 * cos_w0) * a0_inv,
            b2: (1.0 - alpha * a) * a0_inv,
            a1: (-2.0 * cos_w0) * a0_inv,
            a2: (1.0 - alpha / a) * a0_inv,
        }
    }
}

/// 3-Band Channel EQ with kill switches
pub struct ChannelEq {
    enabled: bool,
    sample_rate: f32,
    // Gain in dB for each band (-12 to +12)
    low_gain: f32,
    mid_gain: f32,
    high_gain: f32,
    // Kill switches (full cut)
    low_kill: bool,
    mid_kill: bool,
    high_kill: bool,
    // Filter coefficients
    low_coeffs: BiquadCoeffs,
    mid_coeffs: BiquadCoeffs,
    high_coeffs: BiquadCoeffs,
    // Per-channel filter states (L, R)
    low_state_l: BiquadState,
    low_state_r: BiquadState,
    mid_state_l: BiquadState,
    mid_state_r: BiquadState,
    high_state_l: BiquadState,
    high_state_r: BiquadState,
    // Crossover frequencies
    low_freq: f32,
    high_freq: f32,
    // Dirty flag for coefficient recalculation
    needs_update: bool,
}

impl ChannelEq {
    const LOW_FREQ: f32 = 250.0;
    const HIGH_FREQ: f32 = 2500.0;
    const MID_Q: f32 = 0.707;
    const KILL_DB: f32 = -60.0;
    const MAX_GAIN_DB: f32 = 12.0;
    const MIN_GAIN_DB: f32 = -12.0;

    pub fn new(sample_rate: f32) -> Self {
        let mut eq = Self {
            enabled: true, // Channel EQ is always on in a DJ mixer
            sample_rate,
            low_gain: 0.0,
            mid_gain: 0.0,
            high_gain: 0.0,
            low_kill: false,
            mid_kill: false,
            high_kill: false,
            low_coeffs: BiquadCoeffs::default(),
            mid_coeffs: BiquadCoeffs::default(),
            high_coeffs: BiquadCoeffs::default(),
            low_state_l: BiquadState::default(),
            low_state_r: BiquadState::default(),
            mid_state_l: BiquadState::default(),
            mid_state_r: BiquadState::default(),
            high_state_l: BiquadState::default(),
            high_state_r: BiquadState::default(),
            low_freq: Self::LOW_FREQ,
            high_freq: Self::HIGH_FREQ,
            needs_update: true,
        };
        eq.update_coefficients();
        eq
    }

    pub fn set_low_gain(&mut self, gain_db: f32) {
        self.low_gain = gain_db.clamp(Self::MIN_GAIN_DB, Self::MAX_GAIN_DB);
        self.needs_update = true;
    }

    pub fn set_mid_gain(&mut self, gain_db: f32) {
        self.mid_gain = gain_db.clamp(Self::MIN_GAIN_DB, Self::MAX_GAIN_DB);
        self.needs_update = true;
    }

    pub fn set_high_gain(&mut self, gain_db: f32) {
        self.high_gain = gain_db.clamp(Self::MIN_GAIN_DB, Self::MAX_GAIN_DB);
        self.needs_update = true;
    }

    pub fn adjust_low(&mut self, delta_db: f32) {
        self.set_low_gain(self.low_gain + delta_db);
    }

    pub fn adjust_mid(&mut self, delta_db: f32) {
        self.set_mid_gain(self.mid_gain + delta_db);
    }

    pub fn adjust_high(&mut self, delta_db: f32) {
        self.set_high_gain(self.high_gain + delta_db);
    }

    pub fn toggle_low_kill(&mut self) {
        self.low_kill = !self.low_kill;
        self.needs_update = true;
    }

    pub fn toggle_mid_kill(&mut self) {
        self.mid_kill = !self.mid_kill;
        self.needs_update = true;
    }

    pub fn toggle_high_kill(&mut self) {
        self.high_kill = !self.high_kill;
        self.needs_update = true;
    }

    pub fn low_gain(&self) -> f32 { self.low_gain }
    pub fn mid_gain(&self) -> f32 { self.mid_gain }
    pub fn high_gain(&self) -> f32 { self.high_gain }
    pub fn low_kill(&self) -> bool { self.low_kill }
    pub fn mid_kill(&self) -> bool { self.mid_kill }
    pub fn high_kill(&self) -> bool { self.high_kill }

    fn effective_low_gain(&self) -> f32 {
        if self.low_kill { Self::KILL_DB } else { self.low_gain }
    }

    fn effective_mid_gain(&self) -> f32 {
        if self.mid_kill { Self::KILL_DB } else { self.mid_gain }
    }

    fn effective_high_gain(&self) -> f32 {
        if self.high_kill { Self::KILL_DB } else { self.high_gain }
    }

    fn update_coefficients(&mut self) {
        self.low_coeffs = BiquadCoeffs::low_shelf(self.low_freq, self.effective_low_gain(), self.sample_rate);
        self.mid_coeffs = BiquadCoeffs::peaking(
            (self.low_freq * self.high_freq).sqrt(), // geometric mean
            self.effective_mid_gain(),
            Self::MID_Q,
            self.sample_rate,
        );
        self.high_coeffs = BiquadCoeffs::high_shelf(self.high_freq, self.effective_high_gain(), self.sample_rate);
        self.needs_update = false;
    }
}

impl Effect for ChannelEq {
    fn process(&mut self, samples: &mut [f32]) {
        if !self.enabled {
            return;
        }

        // Check if EQ is flat (all gains at 0, no kills) - skip processing
        let is_flat = self.low_gain.abs() < 0.01
            && self.mid_gain.abs() < 0.01
            && self.high_gain.abs() < 0.01
            && !self.low_kill
            && !self.mid_kill
            && !self.high_kill;
        if is_flat {
            return;
        }

        if self.needs_update {
            self.update_coefficients();
        }

        let lc = self.low_coeffs;
        let mc = self.mid_coeffs;
        let hc = self.high_coeffs;

        for frame in samples.chunks_mut(2) {
            if frame.len() == 2 {
                // Low shelf
                let l = self.low_state_l.process(frame[0], lc.b0, lc.b1, lc.b2, lc.a1, lc.a2);
                let r = self.low_state_r.process(frame[1], lc.b0, lc.b1, lc.b2, lc.a1, lc.a2);
                // Mid peaking
                let l = self.mid_state_l.process(l, mc.b0, mc.b1, mc.b2, mc.a1, mc.a2);
                let r = self.mid_state_r.process(r, mc.b0, mc.b1, mc.b2, mc.a1, mc.a2);
                // High shelf
                frame[0] = self.high_state_l.process(l, hc.b0, hc.b1, hc.b2, hc.a1, hc.a2);
                frame[1] = self.high_state_r.process(r, hc.b0, hc.b1, hc.b2, hc.a1, hc.a2);
            }
        }
    }

    fn reset(&mut self) {
        self.low_state_l.reset();
        self.low_state_r.reset();
        self.mid_state_l.reset();
        self.mid_state_r.reset();
        self.high_state_l.reset();
        self.high_state_r.reset();
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.reset();
        }
    }

    fn name(&self) -> &'static str {
        "ChannelEQ"
    }
}
