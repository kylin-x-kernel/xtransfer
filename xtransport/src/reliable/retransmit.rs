//! Retransmission management for reliable delivery.
//!
//! This module handles the tracking and scheduling of frame
//! retransmissions when acknowledgments are not received.

use crate::error::{Error, Result};

/// Statistics about retransmission behavior.
#[derive(Debug, Default, Clone, Copy)]
pub struct RetransmitStats {
    /// Total frames sent.
    pub frames_sent: u64,

    /// Total retransmissions.
    pub retransmissions: u64,

    /// Frames that exceeded max retransmit.
    pub failed_frames: u64,

    /// Successful deliveries (received ACK).
    pub successful_deliveries: u64,
}

impl RetransmitStats {
    /// Creates new empty statistics.
    pub const fn new() -> Self {
        Self {
            frames_sent: 0,
            retransmissions: 0,
            failed_frames: 0,
            successful_deliveries: 0,
        }
    }

    /// Returns the retransmission rate as a percentage.
    pub fn retransmit_rate(&self) -> f32 {
        if self.frames_sent == 0 {
            0.0
        } else {
            (self.retransmissions as f32 / self.frames_sent as f32) * 100.0
        }
    }

    /// Returns the success rate as a percentage.
    pub fn success_rate(&self) -> f32 {
        let total = self.successful_deliveries + self.failed_frames;
        if total == 0 {
            100.0
        } else {
            (self.successful_deliveries as f32 / total as f32) * 100.0
        }
    }

    /// Resets all statistics.
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

/// Entry tracking a frame pending acknowledgment.
#[derive(Debug, Clone, Copy)]
pub struct PendingFrame {
    /// Sequence number of the frame.
    pub sequence: u32,

    /// Timestamp when frame was last sent.
    pub last_sent: u64,

    /// Number of transmission attempts.
    pub attempts: u8,

    /// Whether this entry is active.
    pub active: bool,
}

impl PendingFrame {
    /// Creates a new pending frame entry.
    pub const fn new(sequence: u32, timestamp: u64) -> Self {
        Self {
            sequence,
            last_sent: timestamp,
            attempts: 1,
            active: true,
        }
    }

    /// Creates an empty entry.
    pub const fn empty() -> Self {
        Self {
            sequence: 0,
            last_sent: 0,
            attempts: 0,
            active: false,
        }
    }
}

/// Retransmission timer using exponential backoff.
#[derive(Debug, Clone, Copy)]
pub struct RetransmitTimer {
    /// Base timeout in milliseconds.
    base_timeout: u64,

    /// Maximum timeout in milliseconds.
    max_timeout: u64,

    /// Current timeout value.
    current_timeout: u64,

    /// Backoff multiplier (e.g., 2 for doubling).
    backoff_factor: u8,
}

impl RetransmitTimer {
    /// Creates a new retransmit timer.
    pub const fn new(base_timeout: u64, max_timeout: u64, backoff_factor: u8) -> Self {
        Self {
            base_timeout,
            max_timeout,
            current_timeout: base_timeout,
            backoff_factor,
        }
    }

    /// Creates a timer with default parameters.
    pub const fn default_timer() -> Self {
        Self::new(1000, 30000, 2)
    }

    /// Returns the current timeout value.
    pub const fn timeout(&self) -> u64 {
        self.current_timeout
    }

    /// Advances the timer after a timeout (exponential backoff).
    pub fn backoff(&mut self) {
        let new_timeout = self.current_timeout.saturating_mul(self.backoff_factor as u64);
        self.current_timeout = core::cmp::min(new_timeout, self.max_timeout);
    }

    /// Resets the timer to base timeout (on successful ACK).
    pub fn reset(&mut self) {
        self.current_timeout = self.base_timeout;
    }

    /// Updates the base timeout based on measured RTT.
    pub fn update_rtt(&mut self, rtt_ms: u64) {
        // Use simple SRTT calculation: base = 1.5 * RTT
        self.base_timeout = rtt_ms + (rtt_ms / 2);
        self.current_timeout = self.base_timeout;
    }
}

impl Default for RetransmitTimer {
    fn default() -> Self {
        Self::default_timer()
    }
}

/// Manager for tracking and scheduling retransmissions.
///
/// This struct maintains a list of frames awaiting acknowledgment
/// and determines when they should be retransmitted.
#[derive(Debug)]
pub struct RetransmitManager<const N: usize> {
    /// Pending frame entries.
    pending: [PendingFrame; N],

    /// Number of active entries.
    count: usize,

    /// Maximum retransmission attempts.
    max_attempts: u8,

    /// Retransmit timer.
    timer: RetransmitTimer,

    /// Statistics.
    stats: RetransmitStats,
}

impl<const N: usize> RetransmitManager<N> {
    /// Creates a new retransmit manager.
    pub fn new(max_attempts: u8, timer: RetransmitTimer) -> Self {
        Self {
            pending: [PendingFrame::empty(); N],
            count: 0,
            max_attempts,
            timer,
            stats: RetransmitStats::new(),
        }
    }

    /// Creates a manager with default settings.
    pub fn with_defaults() -> Self {
        Self::new(5, RetransmitTimer::default_timer())
    }

    /// Registers a newly sent frame for tracking.
    pub fn register(&mut self, sequence: u32, timestamp: u64) -> Result<()> {
        // Find free slot
        for entry in &mut self.pending {
            if !entry.active {
                *entry = PendingFrame::new(sequence, timestamp);
                self.count += 1;
                self.stats.frames_sent += 1;
                return Ok(());
            }
        }

        Err(Error::BufferFull)
    }

    /// Acknowledges a frame, removing it from tracking.
    ///
    /// Returns the round-trip time if available.
    pub fn acknowledge(&mut self, sequence: u32, current_time: u64) -> Option<u64> {
        for entry in &mut self.pending {
            if entry.active && entry.sequence == sequence {
                let rtt = current_time.saturating_sub(entry.last_sent);
                entry.active = false;
                self.count = self.count.saturating_sub(1);
                self.stats.successful_deliveries += 1;

                // Update RTT estimate
                self.timer.update_rtt(rtt);
                self.timer.reset();

                return Some(rtt);
            }
        }
        None
    }

    /// Acknowledges all frames up to and including sequence number.
    pub fn acknowledge_cumulative(&mut self, ack_seq: u32, current_time: u64) {
        // Collect sequences to acknowledge to avoid borrow issues
        let mut to_ack = heapless::Vec::<u32, 64>::new();
        
        for entry in &self.pending {
            if entry.active {
                // Check if entry.sequence <= ack_seq (handling wrap)
                let diff = ack_seq.wrapping_sub(entry.sequence);
                if diff < 0x7FFFFFFF {
                    let _ = to_ack.push(entry.sequence);
                }
            }
        }
        
        // Now acknowledge each one
        for seq in to_ack {
            self.acknowledge(seq, current_time);
        }
    }

    /// Finds frames that need retransmission.
    ///
    /// Calls the callback for each frame needing retransmission.
    /// Returns the number of frames needing retransmission.
    pub fn check_timeouts<F>(&mut self, current_time: u64, mut callback: F) -> usize
    where
        F: FnMut(u32, bool), // (sequence, exceeded_max)
    {
        let timeout = self.timer.timeout();
        let mut retransmit_count = 0;

        for entry in &mut self.pending {
            if entry.active {
                let elapsed = current_time.saturating_sub(entry.last_sent);

                if elapsed >= timeout {
                    let exceeded = entry.attempts >= self.max_attempts;
                    callback(entry.sequence, exceeded);

                    if exceeded {
                        entry.active = false;
                        self.count = self.count.saturating_sub(1);
                        self.stats.failed_frames += 1;
                    } else {
                        retransmit_count += 1;
                    }
                }
            }
        }

        if retransmit_count > 0 {
            self.timer.backoff();
        }

        retransmit_count
    }

    /// Marks a frame as retransmitted.
    pub fn mark_retransmitted(&mut self, sequence: u32, timestamp: u64) -> Result<()> {
        for entry in &mut self.pending {
            if entry.active && entry.sequence == sequence {
                entry.last_sent = timestamp;
                entry.attempts += 1;
                self.stats.retransmissions += 1;
                return Ok(());
            }
        }
        Err(Error::SequenceOutOfRange)
    }

    /// Returns the number of pending frames.
    pub const fn pending_count(&self) -> usize {
        self.count
    }

    /// Returns the current timeout value.
    pub const fn current_timeout(&self) -> u64 {
        self.timer.timeout()
    }

    /// Returns the statistics.
    pub const fn stats(&self) -> &RetransmitStats {
        &self.stats
    }

    /// Resets the manager to initial state.
    pub fn reset(&mut self) {
        self.pending = [PendingFrame::empty(); N];
        self.count = 0;
        self.timer.reset();
    }

    /// Gets the next deadline when a retransmission might be needed.
    pub fn next_deadline(&self) -> Option<u64> {
        let timeout = self.timer.timeout();

        self.pending
            .iter()
            .filter(|e| e.active)
            .map(|e| e.last_sent.saturating_add(timeout))
            .min()
    }

    /// Returns an iterator over pending sequences.
    pub fn pending_sequences(&self) -> impl Iterator<Item = u32> + '_ {
        self.pending
            .iter()
            .filter(|e| e.active)
            .map(|e| e.sequence)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_acknowledge() {
        let mut manager: RetransmitManager<16> = RetransmitManager::with_defaults();

        manager.register(0, 100).unwrap();
        manager.register(1, 110).unwrap();

        assert_eq!(manager.pending_count(), 2);

        let rtt = manager.acknowledge(0, 150);
        assert_eq!(rtt, Some(50));
        assert_eq!(manager.pending_count(), 1);
    }

    #[test]
    fn test_timeout_detection() {
        let mut manager: RetransmitManager<16> =
            RetransmitManager::new(3, RetransmitTimer::new(100, 1000, 2));

        manager.register(0, 0).unwrap();

        // No timeout yet
        let mut timeout_count = 0;
        manager.check_timeouts(50, |_seq, _exceeded| timeout_count += 1);
        assert_eq!(timeout_count, 0);

        // Now timeout
        manager.check_timeouts(200, |seq, exceeded| {
            assert_eq!(seq, 0);
            assert!(!exceeded);
            timeout_count += 1;
        });
        assert_eq!(timeout_count, 1);
    }

    #[test]
    fn test_max_retransmit() {
        let mut manager: RetransmitManager<16> =
            RetransmitManager::new(2, RetransmitTimer::new(100, 1000, 2));

        manager.register(0, 0).unwrap();

        // First timeout
        manager.mark_retransmitted(0, 100).unwrap();

        // Second timeout (should trigger max exceeded)
        manager.mark_retransmitted(0, 200).unwrap();

        let mut exceeded = false;
        manager.check_timeouts(500, |_, ex| exceeded = ex);
        assert!(exceeded);
    }

    #[test]
    fn test_exponential_backoff() {
        let mut timer = RetransmitTimer::new(100, 1000, 2);

        assert_eq!(timer.timeout(), 100);

        timer.backoff();
        assert_eq!(timer.timeout(), 200);

        timer.backoff();
        assert_eq!(timer.timeout(), 400);

        // Test max cap
        timer.backoff();
        timer.backoff();
        timer.backoff();
        assert_eq!(timer.timeout(), 1000); // Capped at max
    }

    #[test]
    fn test_cumulative_ack() {
        let mut manager: RetransmitManager<16> = RetransmitManager::with_defaults();

        manager.register(0, 100).unwrap();
        manager.register(1, 110).unwrap();
        manager.register(2, 120).unwrap();

        assert_eq!(manager.pending_count(), 3);

        // ACK up to sequence 1
        manager.acknowledge_cumulative(1, 200);
        assert_eq!(manager.pending_count(), 1);

        // Sequence 2 should still be pending
        let seqs: heapless::Vec<u32, 16> = manager.pending_sequences().collect();
        assert_eq!(seqs.len(), 1);
        assert!(seqs.contains(&2));
    }
}
