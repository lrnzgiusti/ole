// Modal input handler - placeholder for more complex multi-key sequences
// The keyboard.rs module handles the basic input; this module will be expanded
// for effect mode multi-key sequences (e.g., d3 = delay level 3)

pub struct ModalInputHandler {
    pending_key: Option<char>,
}

impl Default for ModalInputHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ModalInputHandler {
    pub fn new() -> Self {
        Self { pending_key: None }
    }

    pub fn set_pending(&mut self, key: char) {
        self.pending_key = Some(key);
    }

    pub fn take_pending(&mut self) -> Option<char> {
        self.pending_key.take()
    }
}
