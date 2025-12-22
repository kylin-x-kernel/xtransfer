//! Sliding window implementation for flow control.
//!
//! The sliding window mechanism provides:
//! - Flow control to prevent overwhelming receivers
//! - Tracking of sent but unacknowledged frames
//! - Support for selective acknowledgments

use crate::error::{Error, Result};

/// Maximum window size supported.
#[allow(dead_code)]
pub const MAX_WINDOW_SIZE: usize = 256;

/// Entry in the send window tracking sent frames.
#[derive(Debug, Clone)]
pub struct WindowEntry<const N: usize> {
    /// Frame data (header + payload).
    pub data: [u8; N],

    /// Actual size of frame data.
    pub size: usize,

    /// Sequence number of this frame.
    pub sequence: u32,

    /// Timestamp when frame was sent (for RTT calculation).
    pub sent_time: u64,

    /// Number of times this frame has been transmitted.
    pub transmit_count: u8,

    /// Whether this entry is in use.
    pub in_use: bool,

    /// Whether an ACK has been received for this frame.
    pub acked: bool,
}

impl<const N: usize> WindowEntry<N> {
    /// Creates a new empty window entry.
    pub const fn new() -> Self {
        Self {
            data: [0u8; N],
            size: 0,
            sequence: 0,
            sent_time: 0,
            transmit_count: 0,
            in_use: false,
            acked: false,
        }
    }

    /// Returns the frame data slice.
    pub fn frame_data(&self) -> &[u8] {
        &self.data[..self.size]
    }
}

impl<const N: usize> Default for WindowEntry<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Send window for tracking outgoing frames.
///
/// The send window maintains a buffer of frames that have been
/// sent but not yet acknowledged. It supports:
/// - Adding new frames to send
/// - Acknowledging received frames
/// - Finding frames that need retransmission
#[derive(Debug)]
pub struct SendWindow<const W: usize, const F: usize> {
    /// Window entries.
    entries: [WindowEntry<F>; W],

    /// Base sequence number (oldest unacknowledged).
    base_seq: u32,

    /// Next sequence number to use.
    next_seq: u32,

    /// Number of frames in flight (sent but not acked).
    in_flight: usize,

    /// Window size limit.
    window_size: usize,
}

impl<const W: usize, const F: usize> SendWindow<W, F> {
    /// Creates a new send window.
    pub fn new(window_size: usize, initial_seq: u32) -> Self {
        Self {
            entries: core::array::from_fn(|_| WindowEntry::new()),
            base_seq: initial_seq,
            next_seq: initial_seq,
            in_flight: 0,
            window_size: core::cmp::min(window_size, W),
        }
    }

    /// Returns the next sequence number to use.
    pub const fn next_sequence(&self) -> u32 {
        self.next_seq
    }

    /// Returns the base (oldest unacked) sequence number.
    pub const fn base_sequence(&self) -> u32 {
        self.base_seq
    }

    /// Returns the number of frames in flight.
    pub const fn in_flight(&self) -> usize {
        self.in_flight
    }

    /// Returns true if the window is full.
    pub const fn is_full(&self) -> bool {
        self.in_flight >= self.window_size
    }

    /// Returns true if the window is empty.
    pub const fn is_empty(&self) -> bool {
        self.in_flight == 0
    }

    /// Returns the number of slots available.
    pub const fn available(&self) -> usize {
        self.window_size.saturating_sub(self.in_flight)
    }

    /// Adds a frame to the send window.
    ///
    /// Returns the assigned sequence number.
    pub fn add_frame(&mut self, frame_data: &[u8], sent_time: u64) -> Result<u32> {
        if self.is_full() {
            return Err(Error::WindowFull);
        }

        if frame_data.len() > F {
            return Err(Error::PayloadTooLarge);
        }

        let seq = self.next_seq;
        let index = (seq as usize) % W;

        let entry = &mut self.entries[index];
        entry.data[..frame_data.len()].copy_from_slice(frame_data);
        entry.size = frame_data.len();
        entry.sequence = seq;
        entry.sent_time = sent_time;
        entry.transmit_count = 1;
        entry.in_use = true;
        entry.acked = false;

        self.next_seq = self.next_seq.wrapping_add(1);
        self.in_flight += 1;

        Ok(seq)
    }

    /// Acknowledges frames up to and including the given sequence number.
    ///
    /// Returns the number of frames acknowledged.
    pub fn ack_cumulative(&mut self, ack_seq: u32) -> usize {
        let mut acked = 0;

        while self.base_seq != self.next_seq {
            // Check if base_seq <= ack_seq (handling wrap-around)
            let diff = ack_seq.wrapping_sub(self.base_seq);
            if diff > 0x7FFFFFFF {
                // base_seq > ack_seq (wrapped)
                break;
            }

            let index = (self.base_seq as usize) % W;
            let entry = &mut self.entries[index];

            if entry.in_use && !entry.acked {
                entry.acked = true;
                entry.in_use = false;
                self.in_flight = self.in_flight.saturating_sub(1);
                acked += 1;
            }

            self.base_seq = self.base_seq.wrapping_add(1);
        }

        acked
    }

    /// Acknowledges a specific sequence number (selective ACK).
    pub fn ack_selective(&mut self, seq: u32) -> bool {
        // Check if seq is in window range
        let diff = seq.wrapping_sub(self.base_seq);
        if diff > 0x7FFFFFFF || diff as usize >= self.window_size {
            return false;
        }

        let index = (seq as usize) % W;
        let entry = &mut self.entries[index];

        if entry.in_use && entry.sequence == seq && !entry.acked {
            entry.acked = true;
            return true;
        }

        false
    }

    /// Finds frames that need retransmission.
    ///
    /// Returns frames that have been sent but not acked and have
    /// exceeded the timeout.
    pub fn find_retransmit(&self, current_time: u64, timeout: u64, max_retransmit: u8) -> RetransmitIter<'_, W, F> {
        RetransmitIter {
            window: self,
            current_seq: self.base_seq,
            current_time,
            timeout,
            max_retransmit,
        }
    }

    /// Gets a frame entry for retransmission.
    pub fn get_entry(&self, seq: u32) -> Option<&WindowEntry<F>> {
        let diff = seq.wrapping_sub(self.base_seq);
        if diff > 0x7FFFFFFF || diff as usize >= self.window_size {
            return None;
        }

        let index = (seq as usize) % W;
        let entry = &self.entries[index];

        if entry.in_use && entry.sequence == seq {
            Some(entry)
        } else {
            None
        }
    }

    /// Marks a frame as retransmitted.
    pub fn mark_retransmitted(&mut self, seq: u32, sent_time: u64) -> Result<()> {
        let diff = seq.wrapping_sub(self.base_seq);
        if diff > 0x7FFFFFFF || diff as usize >= self.window_size {
            return Err(Error::SequenceOutOfRange);
        }

        let index = (seq as usize) % W;
        let entry = &mut self.entries[index];

        if entry.in_use && entry.sequence == seq {
            entry.transmit_count += 1;
            entry.sent_time = sent_time;
            Ok(())
        } else {
            Err(Error::SequenceOutOfRange)
        }
    }

    /// Resets the window to initial state.
    pub fn reset(&mut self, initial_seq: u32) {
        for entry in &mut self.entries {
            *entry = WindowEntry::new();
        }
        self.base_seq = initial_seq;
        self.next_seq = initial_seq;
        self.in_flight = 0;
    }
}

/// Iterator over frames needing retransmission.
pub struct RetransmitIter<'a, const W: usize, const F: usize> {
    window: &'a SendWindow<W, F>,
    current_seq: u32,
    current_time: u64,
    timeout: u64,
    max_retransmit: u8,
}

impl<'a, const W: usize, const F: usize> Iterator for RetransmitIter<'a, W, F> {
    type Item = (u32, bool); // (sequence, exceeded_max_retransmit)

    fn next(&mut self) -> Option<Self::Item> {
        while self.current_seq != self.window.next_seq {
            let seq = self.current_seq;
            self.current_seq = self.current_seq.wrapping_add(1);

            let index = (seq as usize) % W;
            let entry = &self.window.entries[index];

            if entry.in_use && !entry.acked && entry.sequence == seq {
                // Check if timeout exceeded
                if self.current_time.saturating_sub(entry.sent_time) >= self.timeout {
                    let exceeded = entry.transmit_count >= self.max_retransmit;
                    return Some((seq, exceeded));
                }
            }
        }
        None
    }
}

/// Receive window for tracking expected incoming frames.
///
/// The receive window tracks which sequence numbers have been
/// received and which are still expected.
#[derive(Debug)]
pub struct ReceiveWindow<const W: usize> {
    /// Bitmap of received sequences within window.
    received: [bool; W],

    /// Expected next sequence number.
    expected_seq: u32,

    /// Highest sequence number received.
    highest_received: u32,

    /// Window size.
    window_size: usize,
}

impl<const W: usize> ReceiveWindow<W> {
    /// Creates a new receive window.
    pub fn new(window_size: usize, initial_seq: u32) -> Self {
        Self {
            received: [false; W],
            expected_seq: initial_seq,
            highest_received: initial_seq.wrapping_sub(1),
            window_size: core::cmp::min(window_size, W),
        }
    }

    /// Returns the expected next sequence number.
    pub const fn expected_sequence(&self) -> u32 {
        self.expected_seq
    }

    /// Returns the highest received sequence number.
    pub const fn highest_received(&self) -> u32 {
        self.highest_received
    }

    /// Checks if a sequence number is within the receive window.
    pub fn is_in_window(&self, seq: u32) -> bool {
        let diff = seq.wrapping_sub(self.expected_seq);
        diff < self.window_size as u32
    }

    /// Marks a sequence as received.
    ///
    /// Returns:
    /// - Ok(true) if this is a new frame
    /// - Ok(false) if this is a duplicate
    /// - Err if sequence is outside window
    pub fn receive(&mut self, seq: u32) -> Result<bool> {
        if !self.is_in_window(seq) {
            // Check if it's an old duplicate (before window)
            let diff = self.expected_seq.wrapping_sub(seq);
            if diff < self.window_size as u32 && diff > 0 {
                return Ok(false); // Old duplicate
            }
            return Err(Error::SequenceOutOfRange);
        }

        let index = (seq as usize) % W;

        if self.received[index] {
            return Ok(false); // Duplicate
        }

        self.received[index] = true;

        // Update highest received
        let diff = seq.wrapping_sub(self.highest_received);
        if diff < 0x7FFFFFFF && diff > 0 {
            self.highest_received = seq;
        }

        Ok(true)
    }

    /// Advances the window, returning the cumulative ACK.
    ///
    /// This should be called after receiving frames to slide
    /// the window forward past contiguously received frames.
    pub fn advance(&mut self) -> u32 {
        while self.received[(self.expected_seq as usize) % W] {
            self.received[(self.expected_seq as usize) % W] = false;
            self.expected_seq = self.expected_seq.wrapping_add(1);
        }

        self.expected_seq
    }

    /// Returns missing sequence numbers for NACK generation.
    pub fn missing_sequences(&self) -> impl Iterator<Item = u32> + '_ {
        let base = self.expected_seq;
        let highest = self.highest_received;

        (0..self.window_size as u32).filter_map(move |offset| {
            let seq = base.wrapping_add(offset);

            // Only report missing sequences up to highest received
            let diff = seq.wrapping_sub(highest);
            if diff > 0x7FFFFFFF {
                // seq <= highest
                let index = (seq as usize) % W;
                if !self.received[index] {
                    return Some(seq);
                }
            }
            None
        })
    }

    /// Resets the window to initial state.
    pub fn reset(&mut self, initial_seq: u32) {
        self.received = [false; W];
        self.expected_seq = initial_seq;
        self.highest_received = initial_seq.wrapping_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_send_window_basic() {
        let mut window: SendWindow<16, 128> = SendWindow::new(8, 0);

        assert_eq!(window.available(), 8);

        let seq = window.add_frame(&[1, 2, 3], 100).unwrap();
        assert_eq!(seq, 0);
        assert_eq!(window.in_flight(), 1);

        let seq = window.add_frame(&[4, 5, 6], 101).unwrap();
        assert_eq!(seq, 1);
        assert_eq!(window.in_flight(), 2);

        // ACK both frames (cumulative ACK up to and including seq 1)
        // ack_cumulative(1) should ACK frames with seq 0 and 1
        let acked = window.ack_cumulative(1);
        assert_eq!(acked, 2);  // Both frames acknowledged
        assert_eq!(window.in_flight(), 0);
    }

    #[test]
    fn test_send_window_full() {
        let mut window: SendWindow<4, 64> = SendWindow::new(4, 0);

        for i in 0..4 {
            window.add_frame(&[i], 0).unwrap();
        }

        assert!(window.is_full());
        assert!(window.add_frame(&[4], 0).is_err());
    }

    #[test]
    fn test_receive_window_basic() {
        let mut window: ReceiveWindow<16> = ReceiveWindow::new(8, 0);

        assert!(window.receive(0).unwrap());
        assert!(!window.receive(0).unwrap()); // Duplicate

        assert!(window.receive(2).unwrap()); // Out of order
        assert!(window.receive(1).unwrap());

        let ack = window.advance();
        assert_eq!(ack, 3);
    }

    #[test]
    fn test_receive_window_out_of_range() {
        let mut window: ReceiveWindow<8> = ReceiveWindow::new(4, 0);

        assert!(window.receive(10).is_err());
    }

    #[test]
    fn test_missing_sequences() {
        let mut window: ReceiveWindow<16> = ReceiveWindow::new(8, 0);

        window.receive(0).unwrap();
        window.receive(2).unwrap();
        window.receive(4).unwrap();

        let missing: heapless::Vec<u32, 16> = window.missing_sequences().collect();
        assert!(missing.contains(&1));
        assert!(missing.contains(&3));
        assert_eq!(missing.len(), 2);
    }
}
