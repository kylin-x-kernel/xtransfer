//! Packet definition for the transport protocol.
//!
//! A packet represents a complete message from the application layer.
//! Packets may be fragmented into multiple frames for transmission
//! and reassembled at the receiver.

use crate::error::{Error, Result};
use crate::MAX_PACKET_SIZE;

/// A packet representing application-level data.
///
/// Packets are the unit of data exposed to applications.
/// The transport layer handles fragmentation and reassembly
/// transparently.
#[derive(Debug, Clone)]
pub struct Packet<'a> {
    /// Unique packet identifier for fragment reassembly.
    pub id: u16,

    /// Packet payload data.
    pub data: &'a [u8],

    /// Timestamp when packet was created (for timeout tracking).
    pub timestamp: u64,
}

impl<'a> Packet<'a> {
    /// Creates a new packet with the given ID and data.
    pub fn new(id: u16, data: &'a [u8]) -> Result<Self> {
        if data.len() > MAX_PACKET_SIZE {
            return Err(Error::PacketTooLarge);
        }

        Ok(Self {
            id,
            data,
            timestamp: 0,
        })
    }

    /// Creates a new packet with timestamp.
    pub fn with_timestamp(id: u16, data: &'a [u8], timestamp: u64) -> Result<Self> {
        if data.len() > MAX_PACKET_SIZE {
            return Err(Error::PacketTooLarge);
        }

        Ok(Self {
            id,
            data,
            timestamp,
        })
    }

    /// Returns the number of fragments needed to transmit this packet.
    ///
    /// # Arguments
    ///
    /// * `max_payload` - Maximum payload size per frame
    pub fn fragment_count(&self, max_payload: usize) -> usize {
        if self.data.is_empty() {
            return 1;
        }
        self.data.len().div_ceil(max_payload)
    }

    /// Returns the payload for a specific fragment.
    ///
    /// # Arguments
    ///
    /// * `index` - Fragment index (0-based)
    /// * `max_payload` - Maximum payload size per frame
    ///
    /// # Returns
    ///
    /// The slice of data for this fragment, or None if index is out of range.
    pub fn fragment_data(&self, index: usize, max_payload: usize) -> Option<&[u8]> {
        let total = self.fragment_count(max_payload);
        if index >= total {
            return None;
        }

        let start = index * max_payload;
        let end = core::cmp::min(start + max_payload, self.data.len());

        Some(&self.data[start..end])
    }

    /// Validates the packet structure.
    pub fn validate(&self) -> Result<()> {
        if self.data.len() > MAX_PACKET_SIZE {
            return Err(Error::PacketTooLarge);
        }
        Ok(())
    }
}

/// Owned packet buffer for storing reassembled data.
///
/// This is used when `alloc` feature is enabled.
#[cfg(feature = "alloc")]
#[derive(Debug, Clone)]
pub struct OwnedPacket {
    /// Unique packet identifier.
    pub id: u16,

    /// Packet payload data.
    pub data: alloc::vec::Vec<u8>,

    /// Timestamp when packet was created.
    pub timestamp: u64,
}

#[cfg(feature = "alloc")]
impl OwnedPacket {
    /// Creates a new owned packet.
    pub fn new(id: u16, data: alloc::vec::Vec<u8>) -> Result<Self> {
        if data.len() > MAX_PACKET_SIZE {
            return Err(Error::PacketTooLarge);
        }

        Ok(Self {
            id,
            data,
            timestamp: 0,
        })
    }

    /// Creates a packet from borrowed data.
    pub fn from_slice(id: u16, data: &[u8]) -> Result<Self> {
        if data.len() > MAX_PACKET_SIZE {
            return Err(Error::PacketTooLarge);
        }

        Ok(Self {
            id,
            data: data.to_vec(),
            timestamp: 0,
        })
    }

    /// Converts to a borrowed Packet reference.
    pub fn as_packet(&self) -> Packet<'_> {
        Packet {
            id: self.id,
            data: &self.data,
            timestamp: self.timestamp,
        }
    }
}

/// Fixed-size packet buffer for no_std environments.
///
/// This struct provides a statically-sized buffer for packet
/// data when dynamic allocation is not available.
#[derive(Debug)]
pub struct PacketBuffer<const N: usize> {
    /// The buffer storing packet data.
    buffer: [u8; N],

    /// Current length of valid data in the buffer.
    len: usize,

    /// Packet ID.
    id: u16,

    /// Timestamp.
    timestamp: u64,
}

impl<const N: usize> PacketBuffer<N> {
    /// Creates a new empty packet buffer.
    pub const fn new() -> Self {
        Self {
            buffer: [0u8; N],
            len: 0,
            id: 0,
            timestamp: 0,
        }
    }

    /// Sets the packet ID.
    pub fn set_id(&mut self, id: u16) {
        self.id = id;
    }

    /// Returns the packet ID.
    pub const fn id(&self) -> u16 {
        self.id
    }

    /// Sets the timestamp.
    pub fn set_timestamp(&mut self, timestamp: u64) {
        self.timestamp = timestamp;
    }

    /// Returns the timestamp.
    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }

    /// Clears the buffer.
    pub fn clear(&mut self) {
        self.len = 0;
        self.id = 0;
        self.timestamp = 0;
    }

    /// Returns the current length of data.
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Returns true if the buffer is empty.
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the buffer capacity.
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Returns the remaining capacity.
    pub const fn remaining(&self) -> usize {
        N - self.len
    }

    /// Returns a slice of the valid data.
    pub fn as_slice(&self) -> &[u8] {
        &self.buffer[..self.len]
    }

    /// Returns a mutable slice of the entire buffer.
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.buffer
    }

    /// Writes data to the buffer at the current position.
    ///
    /// Returns the number of bytes written.
    pub fn write(&mut self, data: &[u8]) -> Result<usize> {
        let to_write = core::cmp::min(data.len(), self.remaining());
        if to_write == 0 && !data.is_empty() {
            return Err(Error::BufferFull);
        }

        self.buffer[self.len..self.len + to_write].copy_from_slice(&data[..to_write]);
        self.len += to_write;

        Ok(to_write)
    }

    /// Writes data at a specific offset.
    ///
    /// Used for out-of-order fragment reassembly.
    pub fn write_at(&mut self, offset: usize, data: &[u8]) -> Result<usize> {
        let end = offset + data.len();
        if end > N {
            return Err(Error::BufferTooSmall);
        }

        self.buffer[offset..end].copy_from_slice(data);

        // Update length if we wrote past the current end
        if end > self.len {
            self.len = end;
        }

        Ok(data.len())
    }

    /// Sets the length of valid data.
    ///
    /// # Safety
    ///
    /// Caller must ensure that data up to `len` is valid.
    pub fn set_len(&mut self, len: usize) -> Result<()> {
        if len > N {
            return Err(Error::BufferTooSmall);
        }
        self.len = len;
        Ok(())
    }

    /// Converts to a Packet reference.
    pub fn as_packet(&self) -> Packet<'_> {
        Packet {
            id: self.id,
            data: self.as_slice(),
            timestamp: self.timestamp,
        }
    }
}

impl<const N: usize> Default for PacketBuffer<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fragment_count() {
        let data = [0u8; 2500];
        let packet = Packet::new(1, &data).unwrap();

        // With 1024 byte max payload: ceil(2500 / 1024) = 3
        assert_eq!(packet.fragment_count(1024), 3);

        // With 512 byte max payload: ceil(2500 / 512) = 5
        assert_eq!(packet.fragment_count(512), 5);
    }

    #[test]
    fn test_fragment_data() {
        // Create data array using a loop
        let mut data = [0u8; 100];
        for i in 0..100 {
            data[i] = i as u8;
        }
        let packet = Packet::new(1, &data).unwrap();

        let frag0 = packet.fragment_data(0, 30).unwrap();
        assert_eq!(frag0.len(), 30);
        assert_eq!(frag0[0], 0);

        let frag3 = packet.fragment_data(3, 30).unwrap();
        assert_eq!(frag3.len(), 10); // Last fragment
        assert_eq!(frag3[0], 90);

        assert!(packet.fragment_data(4, 30).is_none());
    }

    #[test]
    fn test_packet_buffer() {
        let mut buf: PacketBuffer<1024> = PacketBuffer::new();

        buf.write(b"Hello, ").unwrap();
        buf.write(b"World!").unwrap();

        assert_eq!(buf.len(), 13);
        assert_eq!(buf.as_slice(), b"Hello, World!");
    }

    #[test]
    fn test_packet_buffer_write_at() {
        let mut buf: PacketBuffer<1024> = PacketBuffer::new();

        // Write out of order (simulating fragment reassembly)
        buf.write_at(10, b"World").unwrap();
        buf.write_at(0, b"Hello ").unwrap();

        // Note: gap between 6-9 contains zeros
        assert_eq!(buf.len(), 15);
    }
}
