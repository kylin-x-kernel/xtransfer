//! Core data structures for the transport protocol.
//!
//! This module contains fundamental building blocks:
//! - Frame: Wire-level protocol unit with header and payload
//! - Packet: Application-level data unit
//! - Checksum: CRC32 for data integrity verification

mod frame;
mod packet;
mod checksum;

pub use frame::{Frame, FrameType, FrameFlags, FRAME_HEADER_SIZE, MAX_FRAME_SIZE};
pub use packet::{Packet, PacketBuffer};
pub use checksum::Crc32;
