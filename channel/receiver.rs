//! Receiver channel implementation with TCP-style ring buffer.
//!
//! This module manages the receive side of the protocol using a
//! byte-stream model similar to TCP, with a ring buffer for
//! data storage and out-of-order segment handling.

extern crate alloc;
use alloc::boxed::Box;
use alloc::vec;

use crate::config::Config;
use crate::error::{Error, Result};

/// Entry tracking an out-of-order segment.
#[derive(Debug, Clone, Copy)]
struct OooSegment {
    /// Starting sequence number.
    seq: u32,
    /// Length of the segment.
    len: u16,
    /// Whether this slot is in use.
    in_use: bool,
}

impl OooSegment {
    const fn new() -> Self {
        Self {
            seq: 0,
            len: 0,
            in_use: false,
        }
    }
}

/// Receiver channel with TCP-style ring buffer.
pub struct Receiver {
    /// Ring buffer for received data (heap allocated).
    buffer: Box<[u8]>,
    
    /// Buffer capacity.
    capacity: usize,
    
    /// Next expected sequence number (recv_next in TCP).
    recv_next: u32,
    
    /// Number of contiguous readable bytes.
    readable: usize,
    
    /// Read position within the readable region.
    read_pos: usize,
    
    /// Out-of-order segments tracking.
    ooo_segments: Box<[OooSegment]>,
    
    /// Bitmap tracking which bytes have been received.
    /// Each bit represents one byte in the buffer.
    recv_bitmap: Box<[u8]>,
}

impl Receiver {
    /// Creates a new receiver with the given configuration.
    pub fn new(initial_seq: u32, config: &Config) -> Self {
        let capacity = config.recv_buffer_size;
        let bitmap_size = (capacity + 7) / 8;
        
        Self {
            buffer: vec![0u8; capacity].into_boxed_slice(),
            capacity,
            recv_next: initial_seq,
            readable: 0,
            read_pos: 0,
            ooo_segments: vec![OooSegment::new(); config.max_segments].into_boxed_slice(),
            recv_bitmap: vec![0u8; bitmap_size].into_boxed_slice(),
        }
    }

    /// Reinitializes the receiver with a new sequence number.
    pub fn reinit(&mut self, initial_seq: u32) {
        self.recv_next = initial_seq;
        self.readable = 0;
        self.read_pos = 0;
        
        // Clear bitmap
        for byte in self.recv_bitmap.iter_mut() {
            *byte = 0;
        }
        
        // Clear OOO segments
        for seg in self.ooo_segments.iter_mut() {
            seg.in_use = false;
        }
    }

    /// Returns the next expected sequence number (for ACK).
    pub fn next_expected(&self) -> u32 {
        self.recv_next
    }

    /// Returns true if data is available for reading.
    pub fn has_data(&self) -> bool {
        self.readable > self.read_pos
    }

    /// Returns the number of readable bytes.
    pub fn readable_len(&self) -> usize {
        self.readable - self.read_pos
    }

    /// Returns the buffer index for a sequence number.
    fn seq_to_index(&self, seq: u32) -> usize {
        (seq as usize) % self.capacity
    }

    /// Sets a bit in the receive bitmap.
    fn set_bitmap(&mut self, offset: usize) {
        let byte_idx = offset / 8;
        let bit_idx = offset % 8;
        if byte_idx < self.recv_bitmap.len() {
            self.recv_bitmap[byte_idx] |= 1 << bit_idx;
        }
    }

    /// Checks if a bit is set in the receive bitmap.
    fn is_bitmap_set(&self, offset: usize) -> bool {
        let byte_idx = offset / 8;
        let bit_idx = offset % 8;
        if byte_idx < self.recv_bitmap.len() {
            (self.recv_bitmap[byte_idx] & (1 << bit_idx)) != 0
        } else {
            false
        }
    }

    /// Clears bitmap bits for bytes that have been read.
    fn clear_bitmap_range(&mut self, start: usize, len: usize) {
        for i in 0..len {
            let offset = (start + i) % self.capacity;
            let byte_idx = offset / 8;
            let bit_idx = offset % 8;
            if byte_idx < self.recv_bitmap.len() {
                self.recv_bitmap[byte_idx] &= !(1 << bit_idx);
            }
        }
    }

    /// Receives a segment and stores it in the buffer.
    ///
    /// Returns the number of new bytes that became readable.
    pub fn receive(&mut self, seq: u32, data: &[u8]) -> Result<usize> {
        if data.is_empty() {
            return Ok(0);
        }

        // Calculate the offset from recv_next
        let offset = seq.wrapping_sub(self.recv_next);
        
        // Check if segment is within window
        if offset >= self.capacity as u32 {
            // Could be old duplicate or too far ahead
            if offset > 0x8000_0000 {
                // Old duplicate (seq < recv_next with wraparound)
                return Ok(0);
            }
            // Too far ahead
            return Err(Error::InvalidSequence);
        }
        
        let offset = offset as usize;
        
        // Check if we have space
        if offset + data.len() > self.capacity {
            return Err(Error::BufferFull);
        }
        
        // Write data to ring buffer
        for (i, &byte) in data.iter().enumerate() {
            let idx = self.seq_to_index(seq.wrapping_add(i as u32));
            self.buffer[idx] = byte;
            self.set_bitmap(idx);
        }
        
        // Track out-of-order segment if not in order
        if offset > 0 {
            // Find or create OOO entry
            let existing = self.ooo_segments.iter_mut().find(|s| {
                s.in_use && s.seq == seq
            });
            
            if existing.is_none() {
                if let Some(slot) = self.ooo_segments.iter_mut().find(|s| !s.in_use) {
                    slot.seq = seq;
                    slot.len = data.len() as u16;
                    slot.in_use = true;
                }
            }
        }
        
        // Update readable count if this segment is in order
        let new_readable = if offset == 0 {
            self.advance_recv_next(data.len())
        } else {
            0
        };
        
        Ok(new_readable)
    }

    /// Advances recv_next and counts new readable bytes.
    fn advance_recv_next(&mut self, initial_len: usize) -> usize {
        let mut advanced = initial_len;
        self.recv_next = self.recv_next.wrapping_add(initial_len as u32);
        self.readable += initial_len;
        
        // Check for OOO segments that are now in order
        loop {
            let mut found = false;
            
            for seg in self.ooo_segments.iter_mut() {
                if seg.in_use && seg.seq == self.recv_next {
                    // This segment is now in order
                    let len = seg.len as usize;
                    self.recv_next = self.recv_next.wrapping_add(len as u32);
                    self.readable += len;
                    advanced += len;
                    seg.in_use = false;
                    found = true;
                    break;
                }
            }
            
            if !found {
                break;
            }
        }
        
        advanced
    }

    /// Reads data from the receive buffer.
    ///
    /// Returns the number of bytes read.
    pub fn read(&mut self, buf: &mut [u8]) -> usize {
        let available = self.readable - self.read_pos;
        let to_read = buf.len().min(available);
        
        if to_read == 0 {
            return 0;
        }
        
        // Calculate starting sequence for read
        let read_seq = self.recv_next.wrapping_sub(self.readable as u32).wrapping_add(self.read_pos as u32);
        
        // Read from ring buffer
        for i in 0..to_read {
            let idx = self.seq_to_index(read_seq.wrapping_add(i as u32));
            buf[i] = self.buffer[idx];
        }
        
        self.read_pos += to_read;
        
        // If we've read everything, reset
        if self.read_pos >= self.readable {
            // Clear bitmap for read bytes
            let start_seq = self.recv_next.wrapping_sub(self.readable as u32);
            let start_idx = self.seq_to_index(start_seq);
            self.clear_bitmap_range(start_idx, self.readable);
            
            self.readable = 0;
            self.read_pos = 0;
        }
        
        to_read
    }
}
