use crate::{Error, error::ErrorKind, Result};
use crate::config::{MAGIC, VERSION, HEADER_SIZE, MESSAGE_HEAD_SIZE};
use alloc::vec::Vec;
use crc32fast::Hasher;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PacketType {
    Data = 0,          // Single packet message
    MessageHead = 1,   // Multi-packet message header
    MessageData = 2,   // Multi-packet message data
    Ack = 3,           // Acknowledgment packet
}

impl PacketType {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(PacketType::Data),
            1 => Some(PacketType::MessageHead),
            2 => Some(PacketType::MessageData),
            3 => Some(PacketType::Ack),
            _ => None,
        }
    }
}

#[repr(C)]
pub struct PacketHeader {
    pub magic: u32,      // 4 bytes
    pub version: u8,     // 1 byte
    pub pkt_type: u8,    // 1 byte - Packet type
    pub seq: u32,        // 4 bytes
    pub length: u16,     // 2 bytes
    pub crc32: u32,      // 4 bytes
}

impl PacketHeader {
    pub fn new(pkt_type: PacketType, seq: u32, length: u16) -> Self {
        PacketHeader {
            magic: MAGIC,
            version: VERSION,
            pkt_type: pkt_type as u8,
            seq,
            length,
            crc32: 0,
        }
    }

    pub fn to_bytes(&self) -> [u8; HEADER_SIZE] {
        let mut buf = [0u8; HEADER_SIZE];
        buf[0..4].copy_from_slice(&self.magic.to_le_bytes());
        buf[4] = self.version;
        buf[5] = self.pkt_type;
        buf[6..10].copy_from_slice(&self.seq.to_le_bytes());
        buf[10..12].copy_from_slice(&self.length.to_le_bytes());
        buf[12..16].copy_from_slice(&self.crc32.to_le_bytes());
        buf
    }

    pub fn from_bytes(buf: &[u8; HEADER_SIZE]) -> Result<Self> {
        let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        if magic != MAGIC {
            return Err(Error::new(ErrorKind::InvalidMagic));
        }

        let version = buf[4];
        if version != VERSION {
            return Err(Error::new(ErrorKind::InvalidVersion));
        }

        let pkt_type = buf[5];
        let seq = u32::from_le_bytes([buf[6], buf[7], buf[8], buf[9]]);
        let length = u16::from_le_bytes([buf[10], buf[11]]);
        let crc32 = u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]);

        Ok(PacketHeader {
            magic,
            version,
            pkt_type,
            seq,
            length,
            crc32,
        })
    }
}

#[repr(C)]
pub struct MessageHead {
    pub total_length: u64,   // 8 bytes - Total message length
    pub message_id: u64,     // 8 bytes - Unique message ID
    pub packet_count: u32,   // 4 bytes - Total packet count
    pub flags: u32,          // 4 bytes - Message flags
    pub reserved: [u8; 8],   // 8 bytes - Reserved for extension
}

impl MessageHead {
    pub fn new(total_length: u64, message_id: u64, packet_count: u32) -> Self {
        MessageHead {
            total_length,
            message_id,
            packet_count,
            flags: 0,
            reserved: [0; 8],
        }
    }

    pub fn to_bytes(&self) -> [u8; MESSAGE_HEAD_SIZE] {
        let mut buf = [0u8; MESSAGE_HEAD_SIZE];
        buf[0..8].copy_from_slice(&self.total_length.to_le_bytes());
        buf[8..16].copy_from_slice(&self.message_id.to_le_bytes());
        buf[16..20].copy_from_slice(&self.packet_count.to_le_bytes());
        buf[20..24].copy_from_slice(&self.flags.to_le_bytes());
        buf[24..32].copy_from_slice(&self.reserved);
        buf
    }

    pub fn from_bytes(buf: &[u8; MESSAGE_HEAD_SIZE]) -> Result<Self> {
        let total_length = u64::from_le_bytes([
            buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
        ]);
        let message_id = u64::from_le_bytes([
            buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
        ]);
        let packet_count = u32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]);
        let flags = u32::from_le_bytes([buf[20], buf[21], buf[22], buf[23]]);
        let mut reserved = [0u8; 8];
        reserved.copy_from_slice(&buf[24..32]);

        Ok(MessageHead {
            total_length,
            message_id,
            packet_count,
            flags,
            reserved,
        })
    }
}

pub struct Packet {
    pub header: PacketHeader,
    pub data: Vec<u8>,
}

impl Packet {
    pub fn new(pkt_type: PacketType, seq: u32, data: Vec<u8>) -> Self {
        let length = data.len() as u16;
        let mut header = PacketHeader::new(pkt_type, seq, length);
        
        // Calculate CRC32
        let mut hasher = Hasher::new();
        hasher.update(&data);
        header.crc32 = hasher.finalize();

        Packet { header, data }
    }

    pub fn verify_crc(&self) -> bool {
        let mut hasher = Hasher::new();
        hasher.update(&self.data);
        let computed_crc = hasher.finalize();
        computed_crc == self.header.crc32
    }
}
