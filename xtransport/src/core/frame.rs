//! Frame definition and serialization for the transport protocol.
//!
//! A frame is the basic unit of transmission in the protocol.
//! Large packets are split into multiple frames for transmission.
//!
//! # Frame Format
//!
//! ```text
//! 0                   1                   2                   3
//! 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |    Version    |     Type      |           Flags              |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                       Sequence Number                         |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                    Acknowledgment Number                      |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |           Packet ID           |  Fragment Idx |  Total Frags  |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |          Payload Length       |           Reserved            |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                          CRC32                                |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! |                          Payload...                           |
//! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//! ```

use super::checksum::Crc32;
use crate::error::{Error, Result};
use crate::VERSION;

/// Frame header size in bytes.
pub const FRAME_HEADER_SIZE: usize = 24;

/// Maximum frame size including header (re-exported from lib).
pub use crate::MAX_FRAME_SIZE;

/// Frame type indicating the purpose of the frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameType {
    /// Data frame carrying payload.
    Data = 0x01,

    /// Acknowledgment frame.
    Ack = 0x02,

    /// Negative acknowledgment (request retransmit).
    Nack = 0x03,

    /// Keep-alive/heartbeat frame.
    Ping = 0x04,

    /// Response to ping.
    Pong = 0x05,

    /// Connection reset.
    Reset = 0x06,

    /// Window update notification.
    WindowUpdate = 0x07,

    /// Synchronization frame (connection setup).
    Sync = 0x08,

    /// Synchronization acknowledgment.
    SyncAck = 0x09,

    /// Graceful close request.
    Fin = 0x0A,

    /// Close acknowledgment.
    FinAck = 0x0B,
}

impl FrameType {
    /// Converts a byte to a FrameType.
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x01 => Some(Self::Data),
            0x02 => Some(Self::Ack),
            0x03 => Some(Self::Nack),
            0x04 => Some(Self::Ping),
            0x05 => Some(Self::Pong),
            0x06 => Some(Self::Reset),
            0x07 => Some(Self::WindowUpdate),
            0x08 => Some(Self::Sync),
            0x09 => Some(Self::SyncAck),
            0x0A => Some(Self::Fin),
            0x0B => Some(Self::FinAck),
            _ => None,
        }
    }

    /// Returns true if this frame type carries payload data.
    pub const fn has_payload(&self) -> bool {
        matches!(self, Self::Data)
    }

    /// Returns true if this frame requires acknowledgment.
    pub const fn requires_ack(&self) -> bool {
        matches!(self, Self::Data | Self::Sync | Self::Fin)
    }
}

/// Frame flags for additional control information.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FrameFlags(u16);

impl FrameFlags {
    /// No flags set.
    pub const NONE: Self = Self(0);

    /// This is the first fragment of a packet.
    pub const FIRST_FRAGMENT: Self = Self(1 << 0);

    /// This is the last fragment of a packet.
    pub const LAST_FRAGMENT: Self = Self(1 << 1);

    /// Urgent data flag.
    pub const URGENT: Self = Self(1 << 2);

    /// Compressed payload.
    pub const COMPRESSED: Self = Self(1 << 3);

    /// Encrypted payload.
    pub const ENCRYPTED: Self = Self(1 << 4);

    /// Creates flags from raw value.
    pub const fn from_bits(bits: u16) -> Self {
        Self(bits)
    }

    /// Returns the raw bits.
    pub const fn bits(&self) -> u16 {
        self.0
    }

    /// Checks if a flag is set.
    pub const fn contains(&self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Sets a flag.
    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }

    /// Clears a flag.
    pub fn remove(&mut self, other: Self) {
        self.0 &= !other.0;
    }

    /// Combines two flag sets.
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// A frame in the transport protocol.
///
/// Frames are the atomic units of transmission. Large packets
/// are fragmented into multiple frames, each with its own
/// sequence number and checksum.
#[derive(Debug, Clone)]
pub struct Frame<'a> {
    /// Protocol version.
    pub version: u8,

    /// Frame type.
    pub frame_type: FrameType,

    /// Frame flags.
    pub flags: FrameFlags,

    /// Sequence number of this frame.
    pub sequence: u32,

    /// Acknowledgment number (next expected sequence).
    pub ack: u32,

    /// Packet ID for fragment reassembly.
    pub packet_id: u16,

    /// Fragment index within the packet (0-based).
    pub fragment_index: u8,

    /// Total number of fragments in the packet.
    pub total_fragments: u8,

    /// CRC32 checksum of header + payload.
    pub checksum: u32,

    /// Frame payload data.
    pub payload: &'a [u8],
}

impl<'a> Frame<'a> {
    /// Creates a new data frame.
    pub fn new_data(
        sequence: u32,
        ack: u32,
        packet_id: u16,
        fragment_index: u8,
        total_fragments: u8,
        payload: &'a [u8],
    ) -> Self {
        let mut flags = FrameFlags::NONE;
        if fragment_index == 0 {
            flags.insert(FrameFlags::FIRST_FRAGMENT);
        }
        if fragment_index == total_fragments - 1 {
            flags.insert(FrameFlags::LAST_FRAGMENT);
        }

        Self {
            version: VERSION,
            frame_type: FrameType::Data,
            flags,
            sequence,
            ack,
            packet_id,
            fragment_index,
            total_fragments,
            checksum: 0, // Computed during serialization
            payload,
        }
    }

    /// Creates an ACK frame.
    pub fn new_ack(ack: u32) -> Self {
        Self {
            version: VERSION,
            frame_type: FrameType::Ack,
            flags: FrameFlags::NONE,
            sequence: 0,
            ack,
            packet_id: 0,
            fragment_index: 0,
            total_fragments: 0,
            checksum: 0,
            payload: &[],
        }
    }

    /// Creates a NACK frame requesting retransmission.
    pub fn new_nack(sequence: u32) -> Self {
        Self {
            version: VERSION,
            frame_type: FrameType::Nack,
            flags: FrameFlags::NONE,
            sequence,
            ack: 0,
            packet_id: 0,
            fragment_index: 0,
            total_fragments: 0,
            checksum: 0,
            payload: &[],
        }
    }

    /// Creates a PING frame.
    pub fn new_ping(sequence: u32) -> Self {
        Self {
            version: VERSION,
            frame_type: FrameType::Ping,
            flags: FrameFlags::NONE,
            sequence,
            ack: 0,
            packet_id: 0,
            fragment_index: 0,
            total_fragments: 0,
            checksum: 0,
            payload: &[],
        }
    }

    /// Creates a PONG frame.
    pub fn new_pong(sequence: u32) -> Self {
        Self {
            version: VERSION,
            frame_type: FrameType::Pong,
            flags: FrameFlags::NONE,
            sequence,
            ack: 0,
            packet_id: 0,
            fragment_index: 0,
            total_fragments: 0,
            checksum: 0,
            payload: &[],
        }
    }

    /// Creates a SYNC frame for connection setup.
    pub fn new_sync(sequence: u32) -> Self {
        Self {
            version: VERSION,
            frame_type: FrameType::Sync,
            flags: FrameFlags::NONE,
            sequence,
            ack: 0,
            packet_id: 0,
            fragment_index: 0,
            total_fragments: 0,
            checksum: 0,
            payload: &[],
        }
    }

    /// Creates a SYNC-ACK frame.
    pub fn new_sync_ack(sequence: u32, ack: u32) -> Self {
        Self {
            version: VERSION,
            frame_type: FrameType::SyncAck,
            flags: FrameFlags::NONE,
            sequence,
            ack,
            packet_id: 0,
            fragment_index: 0,
            total_fragments: 0,
            checksum: 0,
            payload: &[],
        }
    }

    /// Creates a FIN frame for graceful close.
    pub fn new_fin(sequence: u32) -> Self {
        Self {
            version: VERSION,
            frame_type: FrameType::Fin,
            flags: FrameFlags::NONE,
            sequence,
            ack: 0,
            packet_id: 0,
            fragment_index: 0,
            total_fragments: 0,
            checksum: 0,
            payload: &[],
        }
    }

    /// Creates a window update frame.
    pub fn new_window_update(window_size: u32) -> Self {
        Self {
            version: VERSION,
            frame_type: FrameType::WindowUpdate,
            flags: FrameFlags::NONE,
            sequence: window_size,
            ack: 0,
            packet_id: 0,
            fragment_index: 0,
            total_fragments: 0,
            checksum: 0,
            payload: &[],
        }
    }

    /// Returns the total size of this frame when serialized.
    pub fn wire_size(&self) -> usize {
        FRAME_HEADER_SIZE + self.payload.len()
    }

    /// Serializes the frame into the provided buffer.
    ///
    /// Returns the number of bytes written.
    pub fn serialize(&self, buf: &mut [u8]) -> Result<usize> {
        let total_size = self.wire_size();
        if buf.len() < total_size {
            return Err(Error::BufferTooSmall);
        }

        // Write header (without checksum first)
        buf[0] = self.version;
        buf[1] = self.frame_type as u8;
        buf[2..4].copy_from_slice(&self.flags.bits().to_be_bytes());
        buf[4..8].copy_from_slice(&self.sequence.to_be_bytes());
        buf[8..12].copy_from_slice(&self.ack.to_be_bytes());
        buf[12..14].copy_from_slice(&self.packet_id.to_be_bytes());
        buf[14] = self.fragment_index;
        buf[15] = self.total_fragments;
        buf[16..18].copy_from_slice(&(self.payload.len() as u16).to_be_bytes());
        buf[18..20].fill(0); // Reserved

        // Copy payload
        if !self.payload.is_empty() {
            buf[FRAME_HEADER_SIZE..total_size].copy_from_slice(self.payload);
        }

        // Compute and write checksum (over header bytes 0-19 and payload)
        let checksum = Crc32::compute_slices(&[&buf[0..20], self.payload]);
        buf[20..24].copy_from_slice(&checksum.to_be_bytes());

        Ok(total_size)
    }

    /// Deserializes a frame from the provided buffer.
    ///
    /// Returns the frame and the number of bytes consumed.
    pub fn deserialize(buf: &'a [u8]) -> Result<(Self, usize)> {
        if buf.len() < FRAME_HEADER_SIZE {
            return Err(Error::BufferTooSmall);
        }

        let version = buf[0];
        if version != VERSION {
            return Err(Error::VersionMismatch);
        }

        let frame_type = FrameType::from_u8(buf[1]).ok_or(Error::InvalidFrame)?;
        let flags = FrameFlags::from_bits(u16::from_be_bytes([buf[2], buf[3]]));
        let sequence = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
        let ack = u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]);
        let packet_id = u16::from_be_bytes([buf[12], buf[13]]);
        let fragment_index = buf[14];
        let total_fragments = buf[15];
        let payload_len = u16::from_be_bytes([buf[16], buf[17]]) as usize;
        let stored_checksum = u32::from_be_bytes([buf[20], buf[21], buf[22], buf[23]]);

        let total_size = FRAME_HEADER_SIZE + payload_len;
        if buf.len() < total_size {
            return Err(Error::BufferTooSmall);
        }

        // Verify checksum
        let computed_checksum = Crc32::compute_slices(&[
            &buf[0..20],
            &buf[FRAME_HEADER_SIZE..total_size],
        ]);

        if stored_checksum != computed_checksum {
            return Err(Error::ChecksumMismatch);
        }

        let payload = &buf[FRAME_HEADER_SIZE..total_size];

        Ok((
            Self {
                version,
                frame_type,
                flags,
                sequence,
                ack,
                packet_id,
                fragment_index,
                total_fragments,
                checksum: stored_checksum,
                payload,
            },
            total_size,
        ))
    }

    /// Validates the frame structure.
    pub fn validate(&self) -> Result<()> {
        if self.version != VERSION {
            return Err(Error::VersionMismatch);
        }

        if self.total_fragments > 0 && self.fragment_index >= self.total_fragments {
            return Err(Error::InvalidFragmentIndex);
        }

        if self.payload.len() > crate::MAX_FRAME_SIZE - FRAME_HEADER_SIZE {
            return Err(Error::PayloadTooLarge);
        }

        Ok(())
    }

    /// Returns true if this is the first fragment of a packet.
    pub fn is_first_fragment(&self) -> bool {
        self.flags.contains(FrameFlags::FIRST_FRAGMENT)
    }

    /// Returns true if this is the last fragment of a packet.
    pub fn is_last_fragment(&self) -> bool {
        self.flags.contains(FrameFlags::LAST_FRAGMENT)
    }

    /// Returns true if this frame is a single-fragment packet.
    pub fn is_single_fragment(&self) -> bool {
        self.total_fragments == 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_roundtrip() {
        let payload = b"Hello, World!";
        let frame = Frame::new_data(1, 0, 100, 0, 1, payload);

        let mut buf = [0u8; 256];
        let size = frame.serialize(&mut buf).unwrap();

        let (decoded, consumed) = Frame::deserialize(&buf[..size]).unwrap();
        assert_eq!(consumed, size);
        assert_eq!(decoded.sequence, 1);
        assert_eq!(decoded.packet_id, 100);
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn test_ack_frame() {
        let frame = Frame::new_ack(42);

        let mut buf = [0u8; 64];
        let size = frame.serialize(&mut buf).unwrap();

        let (decoded, _) = Frame::deserialize(&buf[..size]).unwrap();
        assert_eq!(decoded.frame_type, FrameType::Ack);
        assert_eq!(decoded.ack, 42);
    }

    #[test]
    fn test_checksum_verification() {
        let frame = Frame::new_data(1, 0, 1, 0, 1, b"test");

        let mut buf = [0u8; 64];
        let size = frame.serialize(&mut buf).unwrap();

        // Corrupt one byte
        buf[FRAME_HEADER_SIZE] ^= 0xFF;

        let result = Frame::deserialize(&buf[..size]);
        assert!(matches!(result, Err(Error::ChecksumMismatch)));
    }
}
