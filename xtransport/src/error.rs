//! Error types for XTransport protocol.
//!
//! This module defines all possible errors that can occur during
//! transport operations.

use core::fmt;

/// Result type alias for XTransport operations.
pub type Result<T> = core::result::Result<T, Error>;

/// Error types for the transport protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Buffer is too small for the operation.
    BufferTooSmall,

    /// Buffer is full and cannot accept more data.
    BufferFull,

    /// Checksum verification failed.
    ChecksumMismatch,

    /// Invalid frame format or corrupted data.
    InvalidFrame,

    /// Invalid packet format.
    InvalidPacket,

    /// Sequence number is out of expected range.
    SequenceOutOfRange,

    /// Frame payload exceeds maximum allowed size.
    PayloadTooLarge,

    /// Packet exceeds maximum allowed size.
    PacketTooLarge,

    /// Connection timeout occurred.
    Timeout,

    /// Maximum retransmission attempts exceeded.
    MaxRetransmitExceeded,

    /// The channel is closed.
    ChannelClosed,

    /// Window is full, cannot send more frames.
    WindowFull,

    /// Duplicate frame received.
    DuplicateFrame,

    /// Transport I/O error occurred.
    IoError,

    /// Protocol version mismatch.
    VersionMismatch,

    /// Invalid state for this operation.
    InvalidState,

    /// Resource temporarily unavailable (would block).
    WouldBlock,

    /// End of stream reached.
    EndOfStream,

    /// Fragment assembly incomplete.
    IncompleteFragment,

    /// Invalid fragment index.
    InvalidFragmentIndex,

    /// Fragment timeout - not all fragments received in time.
    FragmentTimeout,
}

impl Error {
    /// Returns a human-readable description of the error.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Error::BufferTooSmall => "buffer too small",
            Error::BufferFull => "buffer full",
            Error::ChecksumMismatch => "checksum mismatch",
            Error::InvalidFrame => "invalid frame",
            Error::InvalidPacket => "invalid packet",
            Error::SequenceOutOfRange => "sequence out of range",
            Error::PayloadTooLarge => "payload too large",
            Error::PacketTooLarge => "packet too large",
            Error::Timeout => "timeout",
            Error::MaxRetransmitExceeded => "max retransmit exceeded",
            Error::ChannelClosed => "channel closed",
            Error::WindowFull => "window full",
            Error::DuplicateFrame => "duplicate frame",
            Error::IoError => "I/O error",
            Error::VersionMismatch => "version mismatch",
            Error::InvalidState => "invalid state",
            Error::WouldBlock => "would block",
            Error::EndOfStream => "end of stream",
            Error::IncompleteFragment => "incomplete fragment",
            Error::InvalidFragmentIndex => "invalid fragment index",
            Error::FragmentTimeout => "fragment timeout",
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {}
