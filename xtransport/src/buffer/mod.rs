//! Buffer management for the transport protocol.
//!
//! This module provides buffer abstractions:
//! - RingBuffer: Circular buffer for efficient FIFO operations
//! - SendWindow: Sliding window for tracking sent frames
//! - ReceiveWindow: Sliding window for tracking received frames

mod ring;
mod window;

pub use ring::RingBuffer;
pub use window::{SendWindow, ReceiveWindow, WindowEntry};
