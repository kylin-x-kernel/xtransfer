//! Sender side of the transport channel.
//!
//! Handles packet fragmentation, frame transmission,
//! and retransmission management.

use crate::config::Config;
use crate::error::{Error, Result};
use crate::core::{Frame, Packet};
use crate::reliable::{RetransmitManager, RetransmitStats, RetransmitTimer};
use crate::transport::Transport;
use crate::buffer::SendWindow;
use super::ChannelState;

/// Sender side of the transport channel.
///
/// Handles packet fragmentation, frame transmission,
/// and retransmission management.
#[derive(Debug)]
pub struct Sender<const W: usize, const F: usize> {
    /// Send window for tracking sent frames.
    window: SendWindow<W, F>,

    /// Retransmit manager.
    retransmit: RetransmitManager<W>,

    /// Next packet ID to assign.
    next_packet_id: u16,

    /// Current acknowledgment number.
    ack_num: u32,

    /// Maximum frame payload size.
    max_payload: usize,

    /// Channel state.
    state: ChannelState,

    /// Temporary frame buffer for serialization.
    frame_buf: [u8; F],
}

impl<const W: usize, const F: usize> Sender<W, F> {
    /// Creates a new sender with the given configuration.
    pub fn new(config: &Config) -> Self {
        let timer = RetransmitTimer::new(
            config.retransmit_timeout_ms as u64,
            config.retransmit_timeout_ms as u64 * 30,
            2,
        );

        Self {
            window: SendWindow::new(config.window_size as usize, 0),
            retransmit: RetransmitManager::new(config.max_retransmit, timer),
            next_packet_id: 0,
            ack_num: 0,
            max_payload: config.max_payload_size(),
            state: ChannelState::Open,
            frame_buf: [0u8; F],
        }
    }

    /// Returns the current channel state.
    pub fn state(&self) -> ChannelState {
        self.state
    }

    /// Returns the number of frames in flight.
    pub fn in_flight(&self) -> usize {
        self.window.in_flight()
    }

    /// Returns true if the sender can accept more data.
    pub fn can_send(&self) -> bool {
        self.state == ChannelState::Open && !self.window.is_full()
    }

    /// Sets the acknowledgment number to include in frames.
    pub fn set_ack(&mut self, ack: u32) {
        self.ack_num = ack;
    }

    /// Sends a packet, fragmenting if necessary.
    ///
    /// Returns the number of frames sent.
    pub fn send_packet<T: Transport>(
        &mut self,
        transport: &mut T,
        data: &[u8],
        timestamp: u64,
    ) -> Result<usize> {
        if self.state != ChannelState::Open {
            return Err(Error::ChannelClosed);
        }

        let packet = Packet::new(self.next_packet_id, data)?;
        let fragment_count = packet.fragment_count(self.max_payload);

        // Check if we have enough window space
        if self.window.available() < fragment_count {
            return Err(Error::WindowFull);
        }

        // Send each fragment
        for i in 0..fragment_count {
            let fragment_data = packet.fragment_data(i, self.max_payload)
                .ok_or(Error::InvalidFragmentIndex)?;

            let frame = Frame::new_data(
                self.window.next_sequence(),
                self.ack_num,
                self.next_packet_id,
                i as u8,
                fragment_count as u8,
                fragment_data,
            );

            // Serialize and send
            let size = frame.serialize(&mut self.frame_buf)?;
            transport.write_all(&self.frame_buf[..size])?;

            // Track in window
            self.window.add_frame(&self.frame_buf[..size], timestamp)?;

            // Register for retransmission tracking
            self.retransmit.register(frame.sequence, timestamp)?;
        }

        self.next_packet_id = self.next_packet_id.wrapping_add(1);

        Ok(fragment_count)
    }

    /// Sends an ACK frame.
    pub fn send_ack<T: Transport>(
        &mut self,
        transport: &mut T,
        ack: u32,
    ) -> Result<()> {
        let frame = Frame::new_ack(ack);
        let size = frame.serialize(&mut self.frame_buf)?;
        transport.write_all(&self.frame_buf[..size])
    }

    /// Sends a NACK frame requesting retransmission.
    pub fn send_nack<T: Transport>(
        &mut self,
        transport: &mut T,
        sequence: u32,
    ) -> Result<()> {
        let frame = Frame::new_nack(sequence);
        let size = frame.serialize(&mut self.frame_buf)?;
        transport.write_all(&self.frame_buf[..size])
    }

    /// Sends a PING frame.
    pub fn send_ping<T: Transport>(
        &mut self,
        transport: &mut T,
        timestamp: u64,
    ) -> Result<u32> {
        let seq = self.window.next_sequence();
        let frame = Frame::new_ping(seq);
        let size = frame.serialize(&mut self.frame_buf)?;
        transport.write_all(&self.frame_buf[..size])?;

        // Track ping for RTT measurement
        self.retransmit.register(seq, timestamp)?;

        Ok(seq)
    }

    /// Sends a PONG frame in response to a PING.
    pub fn send_pong<T: Transport>(
        &mut self,
        transport: &mut T,
        sequence: u32,
    ) -> Result<()> {
        let frame = Frame::new_pong(sequence);
        let size = frame.serialize(&mut self.frame_buf)?;
        transport.write_all(&self.frame_buf[..size])
    }

    /// Processes a cumulative ACK.
    pub fn process_ack(&mut self, ack: u32, timestamp: u64) {
        self.window.ack_cumulative(ack);
        self.retransmit.acknowledge_cumulative(ack, timestamp);
    }

    /// Processes a selective ACK.
    pub fn process_selective_ack(&mut self, sequence: u32, timestamp: u64) {
        self.window.ack_selective(sequence);
        self.retransmit.acknowledge(sequence, timestamp);
    }

    /// Checks for retransmissions needed.
    ///
    /// Returns frames that need to be retransmitted.
    pub fn check_retransmit<T: Transport>(
        &mut self,
        transport: &mut T,
        timestamp: u64,
    ) -> Result<usize> {
        let mut retransmit_count = 0;

        self.retransmit.check_timeouts(timestamp, |seq, exceeded| {
            if exceeded {
                // Max retransmit exceeded - frame is lost
                return;
            }

            // Get frame data from window and retransmit
            if let Some(entry) = self.window.get_entry(seq) {
                let data = entry.frame_data();
                if transport.write_all(data).is_ok() {
                    retransmit_count += 1;
                }
            }
        });

        // Mark frames as retransmitted
        for seq in self.retransmit.pending_sequences().collect::<heapless::Vec<u32, 64>>() {
            let _ = self.retransmit.mark_retransmitted(seq, timestamp);
            let _ = self.window.mark_retransmitted(seq, timestamp);
        }

        Ok(retransmit_count)
    }

    /// Initiates graceful close.
    pub fn close<T: Transport>(
        &mut self,
        transport: &mut T,
    ) -> Result<()> {
        if self.state != ChannelState::Open {
            return Ok(());
        }

        let frame = Frame::new_fin(self.window.next_sequence());
        let size = frame.serialize(&mut self.frame_buf)?;
        transport.write_all(&self.frame_buf[..size])?;

        self.state = ChannelState::Closing;
        Ok(())
    }

    /// Returns retransmission statistics.
    pub fn stats(&self) -> &RetransmitStats {
        self.retransmit.stats()
    }

    /// Resets the sender to initial state.
    pub fn reset(&mut self) {
        self.window.reset(0);
        self.retransmit.reset();
        self.next_packet_id = 0;
        self.ack_num = 0;
        self.state = ChannelState::Open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::LoopbackTransport;

    #[test]
    fn test_sender_send_small_packet() {
        let config = Config::default();
        let mut sender: Sender<64, 2048> = Sender::new(&config);
        let mut transport: LoopbackTransport<4096> = LoopbackTransport::new();

        let result = sender.send_packet(&mut transport, b"Hello", 0);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1); // Single fragment
        assert_eq!(sender.in_flight(), 1);
    }

    #[test]
    fn test_sender_send_large_packet() {
        let config = Config::default().with_max_frame_size(34); // 10 payload + 24 header
        let mut sender: Sender<64, 256> = Sender::new(&config);
        let mut transport: LoopbackTransport<4096> = LoopbackTransport::new();

        let data = [0u8; 25];
        let result = sender.send_packet(&mut transport, &data, 0);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 3); // 3 fragments
    }

    #[test]
    fn test_sender_ack_processing() {
        let config = Config::default();
        let mut sender: Sender<64, 2048> = Sender::new(&config);
        let mut transport: LoopbackTransport<4096> = LoopbackTransport::new();

        sender.send_packet(&mut transport, b"Hello", 0).unwrap();
        sender.send_packet(&mut transport, b"World", 0).unwrap();

        assert_eq!(sender.in_flight(), 2);

        // Cumulative ACK up to seq 1 (acknowledges both frames 0 and 1)
        sender.process_ack(1, 100);
        assert_eq!(sender.in_flight(), 0);
    }
}
