//! Configuration for the XTransport protocol.
//!
//! This module provides configuration options for tuning
//! protocol behavior such as window size, timeouts, and buffer sizes.

use crate::core::FRAME_HEADER_SIZE;
use crate::{DEFAULT_MAX_RETRANSMIT, DEFAULT_RETRANSMIT_TIMEOUT_MS, DEFAULT_WINDOW_SIZE, MAX_FRAME_SIZE};

/// Configuration for the transport protocol.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    /// Maximum frame size including header (must be > 24 bytes, default: 1048 bytes).
    pub max_frame_size: usize,

    /// Sliding window size for flow control (default: 64).
    pub window_size: u16,

    /// Retransmit timeout in milliseconds (default: 1000ms).
    pub retransmit_timeout_ms: u32,

    /// Maximum number of retransmission attempts (default: 5).
    pub max_retransmit: u8,

    /// Enable checksum verification (default: true).
    pub enable_checksum: bool,

    /// Fragment reassembly timeout in milliseconds (default: 5000ms).
    pub fragment_timeout_ms: u32,

    /// Maximum number of pending fragments per packet (default: 256).
    pub max_pending_fragments: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_frame_size: MAX_FRAME_SIZE,
            window_size: DEFAULT_WINDOW_SIZE,
            retransmit_timeout_ms: DEFAULT_RETRANSMIT_TIMEOUT_MS,
            max_retransmit: DEFAULT_MAX_RETRANSMIT,
            enable_checksum: true,
            fragment_timeout_ms: 5000,
            max_pending_fragments: 256,
        }
    }
}

impl Config {
    /// Creates a new configuration with default values.
    pub const fn new() -> Self {
        Self {
            max_frame_size: MAX_FRAME_SIZE,
            window_size: DEFAULT_WINDOW_SIZE,
            retransmit_timeout_ms: DEFAULT_RETRANSMIT_TIMEOUT_MS,
            max_retransmit: DEFAULT_MAX_RETRANSMIT,
            enable_checksum: true,
            fragment_timeout_ms: 5000,
            max_pending_fragments: 256,
        }
    }

    /// Sets the maximum frame size (including header).
    ///
    /// # Panics
    ///
    /// Panics if size <= FRAME_HEADER_SIZE (24 bytes).
    pub const fn with_max_frame_size(mut self, size: usize) -> Self {
        assert!(size > FRAME_HEADER_SIZE, "max_frame_size must be > 24 (FRAME_HEADER_SIZE)");
        self.max_frame_size = size;
        self
    }

    /// Returns the maximum payload size (frame size - header).
    pub const fn max_payload_size(&self) -> usize {
        self.max_frame_size - FRAME_HEADER_SIZE
    }

    /// Sets the sliding window size.
    pub const fn with_window_size(mut self, size: u16) -> Self {
        self.window_size = size;
        self
    }

    /// Sets the retransmit timeout in milliseconds.
    pub const fn with_retransmit_timeout_ms(mut self, ms: u32) -> Self {
        self.retransmit_timeout_ms = ms;
        self
    }

    /// Sets the maximum number of retransmission attempts.
    pub const fn with_max_retransmit(mut self, count: u8) -> Self {
        self.max_retransmit = count;
        self
    }

    /// Enables or disables checksum verification.
    pub const fn with_checksum(mut self, enable: bool) -> Self {
        self.enable_checksum = enable;
        self
    }

    /// Sets the fragment reassembly timeout in milliseconds.
    pub const fn with_fragment_timeout_ms(mut self, ms: u32) -> Self {
        self.fragment_timeout_ms = ms;
        self
    }

    /// Creates a configuration optimized for low latency.
    pub const fn low_latency() -> Self {
        Self {
            max_frame_size: 536,  // 512 payload + 24 header
            window_size: 32,
            retransmit_timeout_ms: 200,
            max_retransmit: 3,
            enable_checksum: true,
            fragment_timeout_ms: 2000,
            max_pending_fragments: 128,
        }
    }

    /// Creates a configuration optimized for high throughput.
    pub const fn high_throughput() -> Self {
        Self {
            max_frame_size: 4120,  // 4096 payload + 24 header
            window_size: 128,
            retransmit_timeout_ms: 2000,
            max_retransmit: 10,
            enable_checksum: true,
            fragment_timeout_ms: 10000,
            max_pending_fragments: 512,
        }
    }
}
