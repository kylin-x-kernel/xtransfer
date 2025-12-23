// Protocol constants
pub const MAGIC: u32 = 0x58545250; // "XTRP"
pub const VERSION: u8 = 0x01;
pub const HEADER_SIZE: usize = 16;
pub const MESSAGE_HEAD_SIZE: usize = 32;
pub const DEFAULT_MAX_PAYLOAD_SIZE: usize = 65536 - HEADER_SIZE;

pub struct TransportConfig {
    pub max_payload_size: usize,
}

impl TransportConfig {
    pub fn new() -> Self {
        Self {
            max_payload_size: DEFAULT_MAX_PAYLOAD_SIZE,
        }
    }

    pub fn with_max_frame_size(mut self, frame_size: usize) -> Self {
        self.max_payload_size = frame_size.saturating_sub(HEADER_SIZE);
        self
    }
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self::new()
    }
}
