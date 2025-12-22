//! Receiver side of the transport channel.
//!
//! Handles frame reception, packet reassembly,
//! and acknowledgment generation.

use crate::config::Config;
use crate::error::{Error, Result};
use crate::core::{Frame, FrameType};
use crate::reliable::Reassembler;
use crate::buffer::ReceiveWindow;
use super::ChannelState;

/// Receiver side of the transport channel.
///
/// Handles frame reception, packet reassembly,
/// and acknowledgment generation.
#[derive(Debug)]
pub struct Receiver<const W: usize, const E: usize, const N: usize> {
    /// Receive window for tracking expected frames.
    window: ReceiveWindow<W>,

    /// Fragment reassembler.
    reassembler: Reassembler<E, N>,

    /// Channel state.
    state: ChannelState,

    /// Received data buffer.
    recv_buf: [u8; N],

    /// Length of data in recv_buf.
    recv_len: usize,

    /// Whether there's a complete packet ready.
    packet_ready: bool,

    /// Current packet ID being received.
    current_packet_id: u16,
}

impl<const W: usize, const E: usize, const N: usize> Receiver<W, E, N> {
    /// Creates a new receiver with the given configuration.
    pub fn new(config: &Config) -> Self {
        Self {
            window: ReceiveWindow::new(config.window_size as usize, 0),
            reassembler: Reassembler::new(
                config.max_payload_size(),
                config.fragment_timeout_ms as u64,
            ),
            state: ChannelState::Open,
            recv_buf: [0u8; N],
            recv_len: 0,
            packet_ready: false,
            current_packet_id: 0,
        }
    }

    /// Returns the current channel state.
    pub fn state(&self) -> ChannelState {
        self.state
    }

    /// Returns the expected sequence number (for ACK).
    pub fn expected_sequence(&self) -> u32 {
        self.window.expected_sequence()
    }

    /// Returns true if there's data ready to read.
    pub fn has_data(&self) -> bool {
        self.packet_ready
    }

    /// Processes a received frame.
    ///
    /// Returns Ok(true) if a complete packet is ready.
    pub fn process_frame(&mut self, frame: &Frame<'_>, timestamp: u64) -> Result<bool> {
        match frame.frame_type {
            FrameType::Data => self.process_data_frame(frame, timestamp),
            FrameType::Fin => {
                self.state = ChannelState::Closing;
                Ok(false)
            }
            FrameType::Reset => {
                self.state = ChannelState::Closed;
                Ok(false)
            }
            _ => Ok(false),
        }
    }

    /// Processes a data frame.
    fn process_data_frame(&mut self, frame: &Frame<'_>, timestamp: u64) -> Result<bool> {
        // Check if sequence is in window
        let is_new = self.window.receive(frame.sequence)?;

        if !is_new {
            // Duplicate frame
            return Ok(false);
        }

        // Process through reassembler
        let completed = self.reassembler.process_frame(frame, timestamp)?;

        if let Some(packet_id) = completed {
            // Single fragment or reassembly complete
            if frame.is_single_fragment() {
                // Copy directly to recv buffer
                if frame.payload.len() > N {
                    return Err(Error::BufferTooSmall);
                }
                self.recv_buf[..frame.payload.len()].copy_from_slice(frame.payload);
                self.recv_len = frame.payload.len();
            } else {
                // Get reassembled data
                self.recv_len = self.reassembler.take_completed(packet_id, &mut self.recv_buf)?;
            }

            self.packet_ready = true;
            self.current_packet_id = packet_id;

            // Advance window
            self.window.advance();

            return Ok(true);
        }

        Ok(false)
    }

    /// Reads received packet data.
    ///
    /// Returns the number of bytes copied.
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if !self.packet_ready {
            return Err(Error::WouldBlock);
        }

        if buf.len() < self.recv_len {
            return Err(Error::BufferTooSmall);
        }

        buf[..self.recv_len].copy_from_slice(&self.recv_buf[..self.recv_len]);
        let len = self.recv_len;

        self.packet_ready = false;
        self.recv_len = 0;

        Ok(len)
    }

    /// Returns missing sequence numbers for NACK generation.
    pub fn missing_sequences(&self) -> impl Iterator<Item = u32> + '_ {
        self.window.missing_sequences()
    }

    /// Cleans up timed out reassembly entries.
    pub fn cleanup(&mut self, timestamp: u64) {
        self.reassembler.cleanup_timeout(timestamp);
    }

    /// Resets the receiver to initial state.
    pub fn reset(&mut self) {
        self.window.reset(0);
        self.state = ChannelState::Open;
        self.recv_len = 0;
        self.packet_ready = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Frame;

    #[test]
    fn test_receiver_single_frame() {
        let config = Config::default();
        let mut receiver: Receiver<64, 16, 4096> = Receiver::new(&config);

        let frame = Frame::new_data(0, 0, 1, 0, 1, b"Hello");

        let result = receiver.process_frame(&frame, 0);
        assert!(result.is_ok());
        assert!(result.unwrap());
        assert!(receiver.has_data());

        let mut buf = [0u8; 32];
        let len = receiver.read(&mut buf).unwrap();
        assert_eq!(len, 5);
        assert_eq!(&buf[..5], b"Hello");
    }
}
