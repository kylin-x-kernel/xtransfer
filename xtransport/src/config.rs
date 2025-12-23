// Protocol constants
pub const MAGIC: u32 = 0x58545250; // "XTRP"
pub const VERSION: u8 = 0x01;
pub const HEADER_SIZE: usize = 16;
pub const MESSAGE_HEAD_SIZE: usize = 32;
const DEFAULT_MAX_FRAME_SIZE: usize = 4096; // 4KB

pub struct TransportConfig {
    pub max_payload_size: usize,
    pub wait_for_ack: bool,
}

impl TransportConfig {
    pub fn new() -> Self {
        Self {
            max_payload_size: DEFAULT_MAX_FRAME_SIZE - HEADER_SIZE,
            wait_for_ack: false,
        }
    }

    pub fn with_max_frame_size(mut self, frame_size: usize) -> Self {
        self.max_payload_size = frame_size.saturating_sub(HEADER_SIZE);
        self
    }

    pub fn with_ack(mut self, wait_for_ack: bool) -> Self {
        self.wait_for_ack = wait_for_ack;
        self
    }
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self::new()
    }
}
