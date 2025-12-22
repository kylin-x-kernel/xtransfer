//! Send and receive channels for the transport protocol.
//!
//! This module provides the sender and receiver abstractions
//! that handle fragmentation, reassembly, and flow control.

mod sender;
mod receiver;

pub use sender::Sender;
pub use receiver::Receiver;

/// Channel state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelState {
    /// Channel is open and operational.
    Open,

    /// Channel is closing (FIN sent).
    Closing,

    /// Channel is closed.
    Closed,
}
