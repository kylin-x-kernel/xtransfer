//! Fragment reassembly for out-of-order packet reconstruction.
//!
//! This module handles the reassembly of fragmented packets,
//! supporting out-of-order arrival and duplicate detection.

use crate::error::{Error, Result};
use crate::core::Frame;

/// State of a fragment slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum FragmentState {
    /// Slot is empty/unused.
    Empty,

    /// Slot contains a fragment.
    Received,
}

/// Information about a packet being reassembled.
#[derive(Debug)]
pub struct ReassemblyEntry<const N: usize> {
    /// Packet ID being reassembled.
    pub packet_id: u16,

    /// Buffer for reassembled data.
    pub buffer: [u8; N],

    /// Current length of reassembled data.
    pub len: usize,

    /// Expected total size (set when last fragment received).
    pub expected_size: Option<usize>,

    /// Total number of fragments expected.
    pub total_fragments: u8,

    /// Bitmap of received fragments.
    pub received_fragments: [bool; 256],

    /// Number of fragments received.
    pub received_count: u8,

    /// Timestamp when first fragment was received.
    pub start_time: u64,

    /// Whether this entry is in use.
    pub in_use: bool,
}

impl<const N: usize> ReassemblyEntry<N> {
    /// Creates a new empty reassembly entry.
    pub const fn new() -> Self {
        Self {
            packet_id: 0,
            buffer: [0u8; N],
            len: 0,
            expected_size: None,
            total_fragments: 0,
            received_fragments: [false; 256],
            received_count: 0,
            start_time: 0,
            in_use: false,
        }
    }

    /// Initializes the entry for a new packet.
    pub fn init(&mut self, packet_id: u16, total_fragments: u8, timestamp: u64) {
        self.packet_id = packet_id;
        self.len = 0;
        self.expected_size = None;
        self.total_fragments = total_fragments;
        self.received_fragments = [false; 256];
        self.received_count = 0;
        self.start_time = timestamp;
        self.in_use = true;
    }

    /// Adds a fragment to this entry.
    ///
    /// Returns true if this completes the packet.
    pub fn add_fragment(
        &mut self,
        fragment_index: u8,
        total_fragments: u8,
        payload: &[u8],
        max_fragment_size: usize,
    ) -> Result<bool> {
        if fragment_index >= total_fragments {
            return Err(Error::InvalidFragmentIndex);
        }

        if self.received_fragments[fragment_index as usize] {
            return Ok(false); // Duplicate fragment
        }

        // Calculate offset for this fragment
        let offset = (fragment_index as usize) * max_fragment_size;
        let end = offset + payload.len();

        if end > N {
            return Err(Error::BufferTooSmall);
        }

        // Copy fragment data
        self.buffer[offset..end].copy_from_slice(payload);

        // Mark as received
        self.received_fragments[fragment_index as usize] = true;
        self.received_count += 1;

        // Update total size if this is the last fragment
        if fragment_index == total_fragments - 1 {
            self.expected_size = Some(end);
        }

        // Check if complete
        Ok(self.received_count == total_fragments)
    }

    /// Returns the reassembled data if complete.
    pub fn get_data(&self) -> Option<&[u8]> {
        if self.received_count == self.total_fragments {
            let size = self.expected_size.unwrap_or(self.len);
            Some(&self.buffer[..size])
        } else {
            None
        }
    }

    /// Clears the entry for reuse.
    pub fn clear(&mut self) {
        self.in_use = false;
        self.len = 0;
        self.expected_size = None;
        self.received_count = 0;
        self.received_fragments = [false; 256];
    }

    /// Checks if reassembly has timed out.
    pub fn is_timed_out(&self, current_time: u64, timeout: u64) -> bool {
        self.in_use && current_time.saturating_sub(self.start_time) >= timeout
    }

    /// Returns the list of missing fragment indices.
    pub fn missing_fragments(&self) -> impl Iterator<Item = u8> + '_ {
        (0..self.total_fragments).filter(|&i| !self.received_fragments[i as usize])
    }
}

impl<const N: usize> Default for ReassemblyEntry<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Fragment reassembler for handling out-of-order packets.
///
/// Maintains multiple reassembly contexts for concurrent
/// packet reassembly.
#[derive(Debug)]
pub struct Reassembler<const E: usize, const N: usize> {
    /// Reassembly entries.
    entries: [ReassemblyEntry<N>; E],

    /// Maximum fragment payload size.
    max_fragment_size: usize,

    /// Timeout for incomplete reassembly.
    timeout: u64,

    /// Number of active entries.
    active_count: usize,
}

impl<const E: usize, const N: usize> Reassembler<E, N> {
    /// Creates a new reassembler.
    pub fn new(max_fragment_size: usize, timeout: u64) -> Self {
        Self {
            entries: core::array::from_fn(|_| ReassemblyEntry::new()),
            max_fragment_size,
            timeout,
            active_count: 0,
        }
    }

    /// Processes an incoming data frame.
    ///
    /// Returns Some(packet_id) if a packet was completed.
    pub fn process_frame(&mut self, frame: &Frame<'_>, timestamp: u64) -> Result<Option<u16>> {
        let packet_id = frame.packet_id;
        let fragment_index = frame.fragment_index;
        let total_fragments = frame.total_fragments;

        // Single-fragment packet - no reassembly needed
        if total_fragments == 1 {
            return Ok(Some(packet_id));
        }

        // Find or create entry for this packet
        let entry_idx = self.find_or_create_entry(packet_id, total_fragments, timestamp)?;

        let entry = &mut self.entries[entry_idx];

        // Add the fragment
        let complete = entry.add_fragment(
            fragment_index,
            total_fragments,
            frame.payload,
            self.max_fragment_size,
        )?;

        if complete {
            Ok(Some(packet_id))
        } else {
            Ok(None)
        }
    }

    /// Finds an existing entry or creates a new one.
    fn find_or_create_entry(
        &mut self,
        packet_id: u16,
        total_fragments: u8,
        timestamp: u64,
    ) -> Result<usize> {
        // First, look for existing entry
        for (i, entry) in self.entries.iter().enumerate() {
            if entry.in_use && entry.packet_id == packet_id {
                return Ok(i);
            }
        }

        // Find free entry
        for (i, entry) in self.entries.iter_mut().enumerate() {
            if !entry.in_use {
                entry.init(packet_id, total_fragments, timestamp);
                self.active_count += 1;
                return Ok(i);
            }
        }

        // No free entries - try to find timed out entry
        for (i, entry) in self.entries.iter_mut().enumerate() {
            if entry.is_timed_out(timestamp, self.timeout) {
                entry.init(packet_id, total_fragments, timestamp);
                // active_count stays same (recycling)
                return Ok(i);
            }
        }

        Err(Error::BufferFull)
    }

    /// Gets the completed packet data.
    ///
    /// After calling this, the entry is freed.
    pub fn get_completed_data(&mut self, packet_id: u16) -> Option<(&[u8], usize)> {
        for entry in &self.entries {
            if entry.in_use && entry.packet_id == packet_id
                && let Some(data) = entry.get_data()
            {
                return Some((data, data.len()));
            }
        }
        None
    }

    /// Copies completed packet data to buffer and frees the entry.
    pub fn take_completed(&mut self, packet_id: u16, buf: &mut [u8]) -> Result<usize> {
        for entry in &mut self.entries {
            if entry.in_use && entry.packet_id == packet_id
                && let Some(data) = entry.get_data()
            {
                if buf.len() < data.len() {
                    return Err(Error::BufferTooSmall);
                }
                let len = data.len();
                buf[..len].copy_from_slice(data);
                entry.clear();
                self.active_count = self.active_count.saturating_sub(1);
                return Ok(len);
            }
        }
        Err(Error::IncompleteFragment)
    }

    /// Frees a reassembly entry.
    pub fn free_entry(&mut self, packet_id: u16) {
        for entry in &mut self.entries {
            if entry.in_use && entry.packet_id == packet_id {
                entry.clear();
                self.active_count = self.active_count.saturating_sub(1);
                break;
            }
        }
    }

    /// Cleans up timed out entries.
    ///
    /// Returns the number of entries cleaned up.
    pub fn cleanup_timeout(&mut self, current_time: u64) -> usize {
        let mut cleaned = 0;

        for entry in &mut self.entries {
            if entry.is_timed_out(current_time, self.timeout) {
                entry.clear();
                self.active_count = self.active_count.saturating_sub(1);
                cleaned += 1;
            }
        }

        cleaned
    }

    /// Returns the number of active reassembly entries.
    pub const fn active_count(&self) -> usize {
        self.active_count
    }

    /// Returns true if there are any incomplete packets.
    pub fn has_pending(&self) -> bool {
        self.active_count > 0
    }

    /// Gets missing fragments for a specific packet.
    pub fn missing_fragments(&self, packet_id: u16) -> Option<impl Iterator<Item = u8> + '_> {
        for entry in &self.entries {
            if entry.in_use && entry.packet_id == packet_id {
                return Some(entry.missing_fragments());
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Frame;

    #[test]
    fn test_single_fragment() {
        let mut reassembler: Reassembler<4, 1024> = Reassembler::new(256, 5000);

        let frame = Frame::new_data(0, 0, 1, 0, 1, b"Hello");

        let result = reassembler.process_frame(&frame, 0).unwrap();
        assert_eq!(result, Some(1));
    }

    #[test]
    fn test_multi_fragment_in_order() {
        let mut reassembler: Reassembler<4, 1024> = Reassembler::new(256, 5000);

        let frame1 = Frame::new_data(0, 0, 1, 0, 2, b"Hello");
        let frame2 = Frame::new_data(1, 0, 1, 1, 2, b" World");

        let result1 = reassembler.process_frame(&frame1, 0).unwrap();
        assert_eq!(result1, None);

        let result2 = reassembler.process_frame(&frame2, 0).unwrap();
        assert_eq!(result2, Some(1));
    }

    #[test]
    fn test_multi_fragment_out_of_order() {
        let mut reassembler: Reassembler<4, 1024> = Reassembler::new(256, 5000);

        // Receive second fragment first
        let frame2 = Frame::new_data(1, 0, 1, 1, 2, b" World");
        let frame1 = Frame::new_data(0, 0, 1, 0, 2, b"Hello");

        let result2 = reassembler.process_frame(&frame2, 0).unwrap();
        assert_eq!(result2, None);

        let result1 = reassembler.process_frame(&frame1, 0).unwrap();
        assert_eq!(result1, Some(1));
    }

    #[test]
    fn test_duplicate_fragment() {
        let mut reassembler: Reassembler<4, 1024> = Reassembler::new(256, 5000);

        let frame1 = Frame::new_data(0, 0, 1, 0, 2, b"Hello");

        reassembler.process_frame(&frame1, 0).unwrap();
        let result = reassembler.process_frame(&frame1, 0).unwrap();
        assert_eq!(result, None); // Duplicate, not completed
    }

    #[test]
    fn test_cleanup_timeout() {
        let mut reassembler: Reassembler<4, 1024> = Reassembler::new(256, 100);

        let frame1 = Frame::new_data(0, 0, 1, 0, 2, b"Hello");
        reassembler.process_frame(&frame1, 0).unwrap();

        assert_eq!(reassembler.active_count(), 1);

        let cleaned = reassembler.cleanup_timeout(200);
        assert_eq!(cleaned, 1);
        assert_eq!(reassembler.active_count(), 0);
    }
}
