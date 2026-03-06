//! Sampler - 8 one-shot/loop sample slots mixed into master output

use std::sync::Arc;

const NUM_SLOTS: usize = 8;

/// A single sampler slot
pub struct SamplerSlot {
    /// Audio samples (interleaved stereo, f32)
    samples: Arc<Vec<f32>>,
    /// Sample rate of loaded audio
    sample_rate: u32,
    /// Name of loaded sample
    pub name: Option<String>,
    /// Current playback position (in stereo frames)
    position: f64,
    /// Is currently playing
    pub playing: bool,
    /// Per-slot volume (0.0 to 2.0)
    pub gain: f32,
    /// Loop enabled
    pub loop_enabled: bool,
}

impl SamplerSlot {
    fn new() -> Self {
        Self {
            samples: Arc::new(Vec::new()),
            sample_rate: 44100,
            name: None,
            position: 0.0,
            playing: false,
            gain: 1.0,
            loop_enabled: false,
        }
    }

    pub fn is_loaded(&self) -> bool {
        !self.samples.is_empty()
    }

    pub fn load(&mut self, samples: Arc<Vec<f32>>, sample_rate: u32, name: Option<String>) {
        self.samples = samples;
        self.sample_rate = sample_rate;
        self.name = name;
        self.position = 0.0;
        self.playing = false;
    }

    pub fn clear(&mut self) {
        self.samples = Arc::new(Vec::new());
        self.name = None;
        self.position = 0.0;
        self.playing = false;
    }

    pub fn trigger(&mut self) {
        if self.is_loaded() {
            self.position = 0.0;
            self.playing = true;
        }
    }

    pub fn stop(&mut self) {
        self.playing = false;
        self.position = 0.0;
    }

    /// Process and mix into output buffer (interleaved stereo)
    fn process(&mut self, output: &mut [f32], target_sample_rate: u32) {
        if !self.playing || self.samples.is_empty() {
            return;
        }

        let speed = self.sample_rate as f64 / target_sample_rate as f64;
        let total_frames = self.samples.len() / 2;

        for frame in output.chunks_mut(2) {
            if frame.len() < 2 {
                break;
            }

            let pos = self.position as usize;
            if pos >= total_frames {
                if self.loop_enabled {
                    self.position -= total_frames as f64;
                } else {
                    self.playing = false;
                    break;
                }
            }

            let idx = (self.position as usize) * 2;
            if idx + 1 < self.samples.len() {
                frame[0] += self.samples[idx] * self.gain;
                frame[1] += self.samples[idx + 1] * self.gain;
            }

            self.position += speed;
        }
    }
}

/// 8-slot sampler
pub struct Sampler {
    pub slots: [SamplerSlot; NUM_SLOTS],
    sample_rate: u32,
}

impl Sampler {
    pub fn new(sample_rate: u32) -> Self {
        Self {
            slots: std::array::from_fn(|_| SamplerSlot::new()),
            sample_rate,
        }
    }

    pub fn load_slot(&mut self, idx: u8, samples: Arc<Vec<f32>>, sample_rate: u32, name: Option<String>) {
        if (idx as usize) < NUM_SLOTS {
            self.slots[idx as usize].load(samples, sample_rate, name);
        }
    }

    pub fn clear_slot(&mut self, idx: u8) {
        if (idx as usize) < NUM_SLOTS {
            self.slots[idx as usize].clear();
        }
    }

    pub fn trigger(&mut self, idx: u8) {
        if (idx as usize) < NUM_SLOTS {
            self.slots[idx as usize].trigger();
        }
    }

    pub fn stop(&mut self, idx: u8) {
        if (idx as usize) < NUM_SLOTS {
            self.slots[idx as usize].stop();
        }
    }

    pub fn set_gain(&mut self, idx: u8, gain: f32) {
        if (idx as usize) < NUM_SLOTS {
            self.slots[idx as usize].gain = gain.clamp(0.0, 2.0);
        }
    }

    pub fn set_loop(&mut self, idx: u8, enabled: bool) {
        if (idx as usize) < NUM_SLOTS {
            self.slots[idx as usize].loop_enabled = enabled;
        }
    }

    /// Mix all playing slots into the output buffer
    pub fn process(&mut self, output: &mut [f32]) {
        let sr = self.sample_rate;
        for slot in &mut self.slots {
            slot.process(output, sr);
        }
    }

    /// Get slot states for GUI: (loaded, playing, loop_enabled, name)
    pub fn slot_states(&self) -> [(bool, bool, bool, Option<String>); NUM_SLOTS] {
        std::array::from_fn(|i| {
            let s = &self.slots[i];
            (s.is_loaded(), s.playing, s.loop_enabled, s.name.clone())
        })
    }
}
