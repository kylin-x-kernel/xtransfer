use crate::protocol::DEFAULT_MAX_PAYLOAD_SIZE;

pub struct TransportConfig {
    pub max_payload_size: usize,
}

impl TransportConfig {
    pub fn new() -> Self {
        Self {
            max_payload_size: DEFAULT_MAX_PAYLOAD_SIZE,
        }
    }

    pub fn with_max_payload_size(mut self, size: usize) -> Self {
        self.max_payload_size = size;
        self
    }
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self::new()
    }
}
