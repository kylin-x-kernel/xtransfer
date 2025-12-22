//! Ring buffer implementation for efficient data buffering.
//!
//! This module provides a fixed-size ring buffer for storing
//! pending data in both send and receive paths.

use crate::error::{Error, Result};

/// A fixed-size ring buffer for efficient FIFO operations.
///
/// The buffer supports both contiguous and wrapped reads/writes,
/// making it suitable for streaming data.
#[derive(Debug)]
pub struct RingBuffer<const N: usize> {
    /// The underlying storage.
    buffer: [u8; N],

    /// Read position (head).
    head: usize,

    /// Write position (tail).
    tail: usize,

    /// Current number of bytes in buffer.
    len: usize,
}

impl<const N: usize> RingBuffer<N> {
    /// Creates a new empty ring buffer.
    pub const fn new() -> Self {
        Self {
            buffer: [0u8; N],
            head: 0,
            tail: 0,
            len: 0,
        }
    }

    /// Returns the number of bytes in the buffer.
    #[inline]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Returns true if the buffer is empty.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns true if the buffer is full.
    #[inline]
    pub const fn is_full(&self) -> bool {
        self.len == N
    }

    /// Returns the buffer capacity.
    #[inline]
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Returns the number of bytes that can be written.
    #[inline]
    pub const fn remaining(&self) -> usize {
        N - self.len
    }

    /// Clears the buffer.
    pub fn clear(&mut self) {
        self.head = 0;
        self.tail = 0;
        self.len = 0;
    }

    /// Writes data to the buffer.
    ///
    /// Returns the number of bytes written.
    pub fn write(&mut self, data: &[u8]) -> usize {
        let to_write = core::cmp::min(data.len(), self.remaining());
        if to_write == 0 {
            return 0;
        }

        // Calculate how much we can write before wrapping
        let first_chunk = core::cmp::min(to_write, N - self.tail);
        self.buffer[self.tail..self.tail + first_chunk].copy_from_slice(&data[..first_chunk]);

        // Handle wrap-around
        if to_write > first_chunk {
            let second_chunk = to_write - first_chunk;
            self.buffer[..second_chunk].copy_from_slice(&data[first_chunk..to_write]);
        }

        self.tail = (self.tail + to_write) % N;
        self.len += to_write;

        to_write
    }

    /// Writes data to the buffer, returning error if not all data fits.
    pub fn write_all(&mut self, data: &[u8]) -> Result<()> {
        if data.len() > self.remaining() {
            return Err(Error::BufferFull);
        }

        self.write(data);
        Ok(())
    }

    /// Reads data from the buffer.
    ///
    /// Returns the number of bytes read.
    pub fn read(&mut self, buf: &mut [u8]) -> usize {
        let to_read = core::cmp::min(buf.len(), self.len);
        if to_read == 0 {
            return 0;
        }

        // Calculate how much we can read before wrapping
        let first_chunk = core::cmp::min(to_read, N - self.head);
        buf[..first_chunk].copy_from_slice(&self.buffer[self.head..self.head + first_chunk]);

        // Handle wrap-around
        if to_read > first_chunk {
            let second_chunk = to_read - first_chunk;
            buf[first_chunk..to_read].copy_from_slice(&self.buffer[..second_chunk]);
        }

        self.head = (self.head + to_read) % N;
        self.len -= to_read;

        to_read
    }

    /// Peeks at data without consuming it.
    ///
    /// Returns the number of bytes copied.
    pub fn peek(&self, buf: &mut [u8]) -> usize {
        let to_read = core::cmp::min(buf.len(), self.len);
        if to_read == 0 {
            return 0;
        }

        let first_chunk = core::cmp::min(to_read, N - self.head);
        buf[..first_chunk].copy_from_slice(&self.buffer[self.head..self.head + first_chunk]);

        if to_read > first_chunk {
            let second_chunk = to_read - first_chunk;
            buf[first_chunk..to_read].copy_from_slice(&self.buffer[..second_chunk]);
        }

        to_read
    }

    /// Skips (consumes) bytes without reading them.
    pub fn skip(&mut self, count: usize) -> usize {
        let to_skip = core::cmp::min(count, self.len);
        self.head = (self.head + to_skip) % N;
        self.len -= to_skip;
        to_skip
    }

    /// Returns contiguous slices for reading.
    ///
    /// Since the buffer may wrap around, this returns two slices.
    /// The second slice may be empty.
    pub fn as_slices(&self) -> (&[u8], &[u8]) {
        if self.len == 0 {
            return (&[], &[]);
        }

        if self.head + self.len <= N {
            // Data is contiguous
            (&self.buffer[self.head..self.head + self.len], &[])
        } else {
            // Data wraps around
            let first_len = N - self.head;
            let second_len = self.len - first_len;
            (&self.buffer[self.head..], &self.buffer[..second_len])
        }
    }

    /// Returns contiguous mutable slices for writing.
    ///
    /// Returns two slices representing available write space.
    pub fn as_mut_slices(&mut self) -> (&mut [u8], &mut [u8]) {
        let available = self.remaining();
        if available == 0 {
            return (&mut [], &mut []);
        }

        if self.tail + available <= N {
            // Space is contiguous
            (&mut self.buffer[self.tail..self.tail + available], &mut [])
        } else {
            // Space wraps around
            let first_len = N - self.tail;
            let (first, second) = self.buffer.split_at_mut(N - first_len);
            let second_len = available - first_len;
            (&mut second[..first_len], &mut first[..second_len])
        }
    }

    /// Advances the write position after external writes.
    ///
    /// # Safety
    ///
    /// Caller must ensure that `count` bytes were actually written.
    pub fn advance_write(&mut self, count: usize) {
        debug_assert!(count <= self.remaining());
        self.tail = (self.tail + count) % N;
        self.len += count;
    }
}

impl<const N: usize> Default for RingBuffer<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_write_read() {
        let mut buf: RingBuffer<64> = RingBuffer::new();

        let written = buf.write(b"Hello");
        assert_eq!(written, 5);
        assert_eq!(buf.len(), 5);

        let mut out = [0u8; 10];
        let read = buf.read(&mut out);
        assert_eq!(read, 5);
        assert_eq!(&out[..5], b"Hello");
        assert!(buf.is_empty());
    }

    #[test]
    fn test_wrap_around() {
        let mut buf: RingBuffer<8> = RingBuffer::new();

        // Fill most of the buffer
        buf.write(b"12345");
        assert_eq!(buf.len(), 5);

        // Read some
        let mut out = [0u8; 3];
        buf.read(&mut out);
        assert_eq!(&out, b"123");
        assert_eq!(buf.len(), 2);

        // Write more (should wrap)
        buf.write(b"ABCDE");
        assert_eq!(buf.len(), 7);

        // Read all
        let mut out = [0u8; 8];
        let read = buf.read(&mut out);
        assert_eq!(read, 7);
        assert_eq!(&out[..7], b"45ABCDE");
    }

    #[test]
    fn test_peek() {
        let mut buf: RingBuffer<32> = RingBuffer::new();
        buf.write(b"Test data");

        let mut out1 = [0u8; 4];
        let mut out2 = [0u8; 4];

        buf.peek(&mut out1);
        buf.peek(&mut out2);

        assert_eq!(&out1, b"Test");
        assert_eq!(&out2, b"Test");
        assert_eq!(buf.len(), 9);
    }

    #[test]
    fn test_full_buffer() {
        let mut buf: RingBuffer<8> = RingBuffer::new();

        let written = buf.write(b"12345678");
        assert_eq!(written, 8);
        assert!(buf.is_full());

        // Cannot write more
        let written = buf.write(b"9");
        assert_eq!(written, 0);
    }

    #[test]
    fn test_as_slices() {
        let mut buf: RingBuffer<16> = RingBuffer::new();

        // Non-wrapped case
        buf.write(b"Hello");
        let (s1, s2) = buf.as_slices();
        assert_eq!(s1, b"Hello");
        assert_eq!(s2, &[]);

        // Clear and create wrap-around scenario
        buf.clear();
        
        // Fill buffer to position near end
        buf.write(b"12345678901234"); // 14 bytes, tail = 14
        buf.skip(12);                  // head = 12, len = 2 ("34" remains)
        buf.write(b"AB");              // tail = (14+2)%16 = 0, len = 4
        
        // Now: head=12, tail=0, len=4
        // Data wraps: positions 12-15 ("3412" wait, original was "34") and positions 0-1 (wait...)
        // Let me recalculate: after skip(12), positions 12-13 contain "34"
        // after write("AB"), positions 14-15 contain "AB" if there's room
        // Actually: tail was 14, write 2 bytes -> positions 14,15 -> "AB", tail becomes 0
        // So data is: 12-13 "34", 14-15 "AB" = "34AB", contiguous
        // head=12, len=4, head+len=16 <= 16, so contiguous
        
        // Let me actually test wrap-around properly:
        buf.clear();
        buf.write(b"12345678901234"); // 14 bytes at 0-13, tail=14
        buf.skip(12);                  // head=12, len=2 (bytes at 12-13 = "34")
        buf.write(b"ABCD");            // 4 bytes at 14-15,0-1 -> "AB" at 14-15, "CD" at 0-1
                                       // tail = (14+4)%16 = 2, len = 6
        
        // Now: head=12, tail=2, len=6
        // Data: positions 12-15 ("34AB") + positions 0-1 ("CD")
        // head + len = 18 > 16, so wrapped
        let (s1, s2) = buf.as_slices();
        assert_eq!(s1, b"34AB");  // positions 12-15
        assert_eq!(s2, b"CD");    // positions 0-1
    }
}
