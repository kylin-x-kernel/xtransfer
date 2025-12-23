//! Sender channel implementation with TCP-style ring buffer.
//!
//! This module manages the send side of the protocol using a
//! byte-stream model similar to TCP, with a ring buffer for
//! data storage and segment metadata for retransmission.

extern crate alloc;
use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::frame::{encode_with_payload, FrameHeader, HEADER_SIZE, CRC_SIZE};
use crate::time::{Duration, Instant};
use crate::transport::Write;

/// Maximum buffer size for transmit operations.
const MAX_TX_BUF_SIZE: usize = HEADER_SIZE + 65536 + CRC_SIZE;

/// Metadata for a segment in flight.
#[derive(Debug, Clone, Copy)]
struct SegmentMeta {
    /// Starting sequence number (byte offset).
    seq: u32,
    /// Length of this segment.
    len: u16,
    /// Time when this segment was last sent.
    sent_at: Instant,
    /// Number of times this segment has been transmitted.
    tx_count: u8,
    /// Whether this slot is in use.
    in_use: bool,
}

impl SegmentMeta {
    const fn new() -> Self {
        Self {
            seq: 0,
            len: 0,
            sent_at: Instant::from_millis(0),
            tx_count: 0,
            in_use: false,
        }
    }
}

/// Sender channel with TCP-style ring buffer.
pub struct Sender {
    /// Ring buffer for data (heap allocated).
    buffer: Box<[u8]>,
    
    /// Buffer capacity.
    capacity: usize,
    
    /// Oldest unacknowledged byte (send_una in TCP).
    send_una: u32,
    
    /// Next byte to send (send_next in TCP).
    send_next: u32,
    
    /// Next byte to write into buffer (send_max in TCP).
    send_max: u32,
    
    /// Segment metadata array (heap allocated).
    segments: Box<[SegmentMeta]>,
    
    /// Maximum number of segments.
    max_segments: usize,
    
    /// Maximum segment size.
    mss: usize,
    
    /// Acknowledgment number to send (next expected from peer).
    ack_num: u32,
    
    /// Current retransmission timeout in milliseconds.
    rto_ms: u64,
    
    /// Minimum RTO.
    rto_min_ms: u64,
    
    /// Maximum RTO.
    rto_max_ms: u64,
    
    /// Maximum retransmission attempts.
    max_retries: u8,
    
    /// Delayed ACK pending.
    ack_pending: bool,
    
    /// Time when ACK became pending.
    ack_pending_since: Instant,
    
    /// Delayed ACK timeout.
    delayed_ack_ms: u64,
    
    /// Transmit buffer for encoding frames.
    tx_buf: Box<[u8]>,
}

impl Sender {
    /// Creates a new sender with the given configuration.
    pub fn new(initial_seq: u32, config: &Config) -> Self {
        let capacity = config.send_buffer_size;
        let max_segments = config.max_segments;
        let mss = config.max_segment_size;
        
        Self {
            buffer: vec![0u8; capacity].into_boxed_slice(),
            capacity,
            send_una: initial_seq,
            send_next: initial_seq,
            send_max: initial_seq,
            segments: vec![SegmentMeta::new(); max_segments].into_boxed_slice(),
            max_segments,
            mss,
            ack_num: 0,
            rto_ms: config.rto_initial_ms,
            rto_min_ms: config.rto_min_ms,
            rto_max_ms: config.rto_max_ms,
            max_retries: config.max_retries,
            ack_pending: false,
            ack_pending_since: Instant::from_millis(0),
            delayed_ack_ms: config.delayed_ack_ms,
            tx_buf: vec![0u8; MAX_TX_BUF_SIZE].into_boxed_slice(),
        }
    }

    /// Reinitializes the sender with a new sequence number.
    pub fn reinit(&mut self, initial_seq: u32, config: &Config) {
        self.send_una = initial_seq;
        self.send_next = initial_seq;
        self.send_max = initial_seq;
        self.rto_ms = config.rto_initial_ms;
        self.rto_min_ms = config.rto_min_ms;
        self.rto_max_ms = config.rto_max_ms;
        self.max_retries = config.max_retries;
        self.delayed_ack_ms = config.delayed_ack_ms;
        self.ack_pending = false;
        
        // Clear all segments
        for seg in self.segments.iter_mut() {
            seg.in_use = false;
        }
    }

    /// Sets the acknowledgment number (next expected from peer).
    pub fn set_ack(&mut self, ack: u32) {
        self.ack_num = ack;
    }

    /// Returns the current acknowledgment number.
    pub fn ack(&self) -> u32 {
        self.ack_num
    }

    /// Returns the next sequence number (byte offset) to be assigned.
    pub fn next_seq(&self) -> u32 {
        self.send_max
    }

    /// Returns the next sequence number to send.
    pub fn send_next(&self) -> u32 {
        self.send_next
    }

    /// Marks an ACK as pending (for delayed ACK).
    pub fn schedule_ack(&mut self, now: Instant) {
        if !self.ack_pending {
            self.ack_pending = true;
            self.ack_pending_since = now;
        }
    }

    /// Returns true if an ACK should be sent now.
    pub fn should_send_ack(&self, now: Instant) -> bool {
        if !self.ack_pending {
            return false;
        }
        if self.delayed_ack_ms == 0 {
            return true;
        }
        self.ack_pending_since.elapsed_since(now, Duration::from_millis(self.delayed_ack_ms))
    }

    /// Clears the pending ACK flag.
    pub fn clear_ack_pending(&mut self) {
        self.ack_pending = false;
    }

    /// Returns the buffer index for a sequence number.
    fn seq_to_index(&self, seq: u32) -> usize {
        (seq as usize) % self.capacity
    }

    /// Returns the number of bytes available in the send buffer.
    pub fn available(&self) -> usize {
        let used = self.send_max.wrapping_sub(self.send_una) as usize;
        self.capacity.saturating_sub(used)
    }

    /// Returns true if the send window is full.
    pub fn is_window_full(&self) -> bool {
        // Check if we have segment slots available
        let segments_in_use = self.segments.iter().filter(|s| s.in_use).count();
        if segments_in_use >= self.max_segments {
            return true;
        }
        
        // Check if we have buffer space
        self.available() == 0
    }

    /// Returns the number of bytes in flight (sent but not acknowledged).
    pub fn in_flight(&self) -> usize {
        self.send_next.wrapping_sub(self.send_una) as usize
    }

    /// Returns the number of bytes pending (written but not yet sent).
    pub fn pending(&self) -> usize {
        self.send_max.wrapping_sub(self.send_next) as usize
    }

    /// Writes data into the send buffer.
    ///
    /// Returns the number of bytes written.
    pub fn write(&mut self, data: &[u8]) -> usize {
        let available = self.available();
        let to_write = data.len().min(available);
        
        if to_write == 0 {
            return 0;
        }
        
        // Write to ring buffer
        for (i, &byte) in data[..to_write].iter().enumerate() {
            let idx = self.seq_to_index(self.send_max.wrapping_add(i as u32));
            self.buffer[idx] = byte;
        }
        
        self.send_max = self.send_max.wrapping_add(to_write as u32);
        to_write
    }

    /// Reads data from the ring buffer at a given sequence number.
    fn read_from_buffer(&self, seq: u32, len: usize) -> Vec<u8> {
        let mut data = vec![0u8; len];
        for i in 0..len {
            let idx = self.seq_to_index(seq.wrapping_add(i as u32));
            data[i] = self.buffer[idx];
        }
        data
    }

    /// Transmits pending data as segments.
    ///
    /// Returns the number of bytes transmitted.
    pub fn transmit_pending<T: Write>(&mut self, transport: &mut T, now: Instant) -> Result<usize> {
        let mut total_sent = 0;
        
        while self.send_next != self.send_max {
            // Find a free segment slot
            let slot = match self.segments.iter().position(|s| !s.in_use) {
                Some(i) => i,
                None => break, // No free slots
            };
            
            // Calculate segment size
            let pending = self.send_max.wrapping_sub(self.send_next) as usize;
            let seg_len = pending.min(self.mss);
            
            // Read data from ring buffer
            let seg_data = self.read_from_buffer(self.send_next, seg_len);
            
            // Build and send frame
            let header = FrameHeader::data(self.send_next, self.ack_num, seg_len as u16);
            let written = encode_with_payload(&header, &seg_data, &mut self.tx_buf)?;
            
            let mut sent = 0;
            while sent < written {
                match transport.write(&self.tx_buf[sent..written]) {
                    Ok(0) => return Err(Error::IoError),
                    Ok(n) => sent += n,
                    Err(Error::WouldBlock) => continue,
                    Err(e) => return Err(e),
                }
            }
            
            // Record segment metadata
            self.segments[slot] = SegmentMeta {
                seq: self.send_next,
                len: seg_len as u16,
                sent_at: now,
                tx_count: 1,
                in_use: true,
            };
            
            self.send_next = self.send_next.wrapping_add(seg_len as u32);
            total_sent += seg_len;
            
            // Clear pending ACK since we piggybacked it
            self.ack_pending = false;
        }
        
        Ok(total_sent)
    }

    /// Retransmits a specific segment.
    pub fn retransmit<T: Write>(&mut self, seq: u32, transport: &mut T, now: Instant) -> Result<usize> {
        // Find the segment
        let slot = match self.segments.iter().position(|s| s.in_use && s.seq == seq) {
            Some(i) => i,
            None => return Err(Error::InvalidSequence),
        };
        
        if self.segments[slot].tx_count >= self.max_retries {
            return Err(Error::MaxRetriesExceeded);
        }
        
        let seg_len = self.segments[slot].len as usize;
        
        // Read data from ring buffer
        let seg_data = self.read_from_buffer(seq, seg_len);
        
        // Build and send frame
        let header = FrameHeader::data(seq, self.ack_num, seg_len as u16);
        let written = encode_with_payload(&header, &seg_data, &mut self.tx_buf)?;
        
        let mut sent = 0;
        while sent < written {
            match transport.write(&self.tx_buf[sent..written]) {
                Ok(0) => return Err(Error::IoError),
                Ok(n) => sent += n,
                Err(Error::WouldBlock) => continue,
                Err(e) => return Err(e),
            }
        }
        
        self.segments[slot].sent_at = now;
        self.segments[slot].tx_count += 1;
        self.ack_pending = false;
        
        Ok(seg_len)
    }

    /// Processes an incoming ACK.
    ///
    /// Returns the number of bytes acknowledged.
    pub fn on_ack(&mut self, ack: u32) -> usize {
        // Calculate how many bytes are being acknowledged
        let ack_diff = ack.wrapping_sub(self.send_una);
        
        // Sanity check: ACK should be within valid range
        let max_valid = self.send_next.wrapping_sub(self.send_una);
        if ack_diff == 0 || ack_diff > max_valid {
            return 0;
        }
        
        // Free all segments that are fully acknowledged
        for seg in self.segments.iter_mut() {
            if seg.in_use {
                let seg_end = seg.seq.wrapping_add(seg.len as u32);
                // If segment end <= ack, it's fully acknowledged
                if seg_end.wrapping_sub(self.send_una) <= ack_diff {
                    seg.in_use = false;
                }
            }
        }
        
        self.send_una = ack;
        ack_diff as usize
    }

    /// Checks for segments that need retransmission.
    pub fn check_timeout(&self, now: Instant) -> Option<u32> {
        let timeout = Duration::from_millis(self.rto_ms);
        
        for seg in self.segments.iter() {
            if seg.in_use && seg.sent_at.elapsed_since(now, timeout) {
                return Some(seg.seq);
            }
        }
        
        None
    }

    /// Resets the sender state.
    pub fn reset(&mut self) {
        self.send_una = 0;
        self.send_next = 0;
        self.send_max = 0;
        self.ack_pending = false;
        
        for seg in self.segments.iter_mut() {
            seg.in_use = false;
        }
    }

    /// Returns the maximum segment size.
    pub fn mss(&self) -> usize {
        self.mss
    }
}
