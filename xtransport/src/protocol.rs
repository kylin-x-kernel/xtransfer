//! Protocol state machine and main API.
//!
//! This module provides the main `Protocol` struct that orchestrates
//! all components for reliable transport.

use crate::channel::{Receiver, Sender};
use crate::config::Config;
use crate::error::{Error, Result};
use crate::core::{Frame, FrameType, FRAME_HEADER_SIZE};
use crate::reliable::RetransmitStats;
use crate::transport::Transport;

/// Protocol connection state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Initial state, not connected.
    Idle,

    /// Initiating connection (SYNC sent).
    Connecting,

    /// Waiting for connection (SYNC received).
    Accepting,

    /// Connection established.
    Connected,

    /// Connection closing.
    Closing,

    /// Connection closed.
    Closed,
}

/// Statistics about protocol operation.
#[derive(Debug, Default, Clone, Copy)]
pub struct ProtocolStats {
    /// Packets sent.
    pub packets_sent: u64,

    /// Packets received.
    pub packets_received: u64,

    /// Frames sent.
    pub frames_sent: u64,

    /// Frames received.
    pub frames_received: u64,

    /// Bytes sent (payload only).
    pub bytes_sent: u64,

    /// Bytes received (payload only).
    pub bytes_received: u64,

    /// Checksum errors detected.
    pub checksum_errors: u64,

    /// Out of order frames.
    pub out_of_order: u64,

    /// Duplicate frames.
    pub duplicates: u64,
}

/// Maximum frame buffer size for sending/receiving.
const MAX_FRAME_BUF: usize = 2048;

/// Window size for send/receive windows.
const WINDOW_SIZE: usize = 64;

/// Reassembly entry count.
const REASSEMBLY_ENTRIES: usize = 8;

/// Maximum reassembly buffer size (reduced to avoid stack overflow).
const REASSEMBLY_BUF_SIZE: usize = 8192;

/// Main protocol handler.
///
/// This struct combines sending and receiving functionality
/// into a single interface for bidirectional communication.
///
/// # Example
///
/// ```rust,ignore
/// use xtransport::{Protocol, Config, Transport};
///
/// let config = Config::default();
/// let mut protocol = Protocol::new(config);
///
/// // Send data
/// protocol.send(&mut transport, b"Hello, World!")?;
///
/// // Receive data
/// let mut buf = [0u8; 1024];
/// let n = protocol.recv(&mut transport, &mut buf)?;
/// ```
pub struct Protocol {
    /// Configuration.
    config: Config,

    /// Sender channel.
    sender: Sender<WINDOW_SIZE, MAX_FRAME_BUF>,

    /// Receiver channel.
    receiver: Receiver<WINDOW_SIZE, REASSEMBLY_ENTRIES, REASSEMBLY_BUF_SIZE>,

    /// Connection state.
    state: ConnectionState,

    /// Initial sequence number for this side.
    local_seq: u32,

    /// Initial sequence number from peer.
    remote_seq: u32,

    /// Frame receive buffer.
    recv_frame_buf: [u8; MAX_FRAME_BUF],

    /// Current read position in frame buffer.
    recv_pos: usize,

    /// Valid data length in frame buffer.
    recv_len: usize,

    /// Protocol statistics.
    stats: ProtocolStats,

    /// Last activity timestamp.
    last_activity: u64,
}

impl Protocol {
    /// Creates a new protocol instance with the given configuration.
    pub fn new(config: Config) -> Self {
        Self {
            sender: Sender::new(&config),
            receiver: Receiver::new(&config),
            config,
            state: ConnectionState::Idle,
            local_seq: 0,
            remote_seq: 0,
            recv_frame_buf: [0u8; MAX_FRAME_BUF],
            recv_pos: 0,
            recv_len: 0,
            stats: ProtocolStats::default(),
            last_activity: 0,
        }
    }

    /// Creates a protocol instance with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(Config::default())
    }

    /// Returns the current connection state.
    pub fn state(&self) -> ConnectionState {
        self.state
    }

    /// Returns true if the connection is established.
    pub fn is_connected(&self) -> bool {
        self.state == ConnectionState::Connected
    }

    /// Returns protocol statistics.
    pub fn stats(&self) -> &ProtocolStats {
        &self.stats
    }

    /// Returns retransmission statistics.
    pub fn retransmit_stats(&self) -> &RetransmitStats {
        self.sender.stats()
    }

    /// Returns the configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Initiates a connection to peer.
    pub fn connect<T: Transport>(&mut self, transport: &mut T, timestamp: u64) -> Result<()> {
        if self.state != ConnectionState::Idle {
            return Err(Error::InvalidState);
        }

        // Send SYNC frame
        let frame = Frame::new_sync(self.local_seq);
        let mut buf = [0u8; FRAME_HEADER_SIZE];
        let size = frame.serialize(&mut buf)?;
        transport.write_all(&buf[..size])?;

        self.state = ConnectionState::Connecting;
        self.last_activity = timestamp;

        Ok(())
    }

    /// Accepts an incoming connection.
    ///
    /// Call this after receiving a SYNC frame.
    pub fn accept<T: Transport>(&mut self, transport: &mut T, timestamp: u64) -> Result<()> {
        if self.state != ConnectionState::Accepting {
            return Err(Error::InvalidState);
        }

        // Send SYNC-ACK frame
        let frame = Frame::new_sync_ack(self.local_seq, self.remote_seq.wrapping_add(1));
        let mut buf = [0u8; FRAME_HEADER_SIZE];
        let size = frame.serialize(&mut buf)?;
        transport.write_all(&buf[..size])?;

        self.state = ConnectionState::Connected;
        self.last_activity = timestamp;

        Ok(())
    }

    /// Sends data to the peer.
    ///
    /// The data may be fragmented into multiple frames if it
    /// exceeds the maximum frame payload size.
    pub fn send<T: Transport>(
        &mut self,
        transport: &mut T,
        data: &[u8],
        timestamp: u64,
    ) -> Result<()> {
        if self.state != ConnectionState::Connected {
            return Err(Error::InvalidState);
        }

        // Set acknowledgment number
        self.sender.set_ack(self.receiver.expected_sequence());

        // Send the packet
        let frames = self.sender.send_packet(transport, data, timestamp)?;

        self.stats.packets_sent += 1;
        self.stats.frames_sent += frames as u64;
        self.stats.bytes_sent += data.len() as u64;
        self.last_activity = timestamp;

        Ok(())
    }

    /// Receives data from the peer.
    ///
    /// Returns the number of bytes received, or an error.
    pub fn recv<T: Transport>(
        &mut self,
        transport: &mut T,
        buf: &mut [u8],
        timestamp: u64,
    ) -> Result<usize> {
        // Process any pending frames
        self.process_incoming(transport, timestamp)?;

        // Check if we have data ready
        if self.receiver.has_data() {
            let len = self.receiver.read(buf)?;
            self.stats.packets_received += 1;
            self.stats.bytes_received += len as u64;
            return Ok(len);
        }

        Err(Error::WouldBlock)
    }

    /// Processes incoming frames from the transport.
    pub fn process_incoming<T: Transport>(
        &mut self,
        transport: &mut T,
        timestamp: u64,
    ) -> Result<()> {
        loop {
            // Try to read frame header
            if self.recv_len < FRAME_HEADER_SIZE {
                let mut header_buf = [0u8; FRAME_HEADER_SIZE];
                match transport.read(&mut header_buf[self.recv_len..FRAME_HEADER_SIZE]) {
                    Ok(0) => return Ok(()), // No more data
                    Ok(n) => {
                        self.recv_frame_buf[self.recv_len..self.recv_len + n]
                            .copy_from_slice(&header_buf[self.recv_len..self.recv_len + n]);
                        self.recv_len += n;
                    }
                    Err(Error::WouldBlock) => return Ok(()),
                    Err(e) => return Err(e),
                }
            }

            if self.recv_len < FRAME_HEADER_SIZE {
                return Ok(());
            }

            // Parse header to get payload length
            let payload_len = u16::from_be_bytes([
                self.recv_frame_buf[16],
                self.recv_frame_buf[17],
            ]) as usize;

            let total_frame_len = FRAME_HEADER_SIZE + payload_len;

            // Read remaining payload if needed
            while self.recv_len < total_frame_len {
                match transport.read(&mut self.recv_frame_buf[self.recv_len..total_frame_len]) {
                    Ok(0) => return Ok(()),
                    Ok(n) => self.recv_len += n,
                    Err(Error::WouldBlock) => return Ok(()),
                    Err(e) => return Err(e),
                }
            }

            // Copy frame data to a temporary buffer to avoid borrow issues
            let mut frame_data = [0u8; MAX_FRAME_BUF];
            frame_data[..total_frame_len].copy_from_slice(&self.recv_frame_buf[..total_frame_len]);

            // Reset buffer for next frame
            self.recv_pos = 0;
            self.recv_len = 0;

            // Parse complete frame
            match Frame::deserialize(&frame_data[..total_frame_len]) {
                Ok((frame, _)) => {
                    self.handle_frame(&frame, transport, timestamp)?;
                    self.stats.frames_received += 1;
                }
                Err(Error::ChecksumMismatch) => {
                    self.stats.checksum_errors += 1;
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Handles a received frame based on its type.
    fn handle_frame<T: Transport>(
        &mut self,
        frame: &Frame<'_>,
        transport: &mut T,
        timestamp: u64,
    ) -> Result<()> {
        self.last_activity = timestamp;

        match frame.frame_type {
            FrameType::Sync => {
                self.remote_seq = frame.sequence;
                self.state = ConnectionState::Accepting;
            }

            FrameType::SyncAck => {
                if self.state == ConnectionState::Connecting {
                    self.remote_seq = frame.sequence;
                    self.state = ConnectionState::Connected;

                    // Send ACK to complete handshake
                    self.sender.send_ack(transport, frame.sequence.wrapping_add(1))?;
                }
            }

            FrameType::Data => {
                let packet_ready = self.receiver.process_frame(frame, timestamp)?;

                // Piggyback ACK
                if packet_ready {
                    self.sender.send_ack(transport, self.receiver.expected_sequence())?;
                }
            }

            FrameType::Ack => {
                self.sender.process_ack(frame.ack, timestamp);
            }

            FrameType::Nack => {
                // Peer is requesting retransmission
                // The sender will handle this through timeout mechanism
            }

            FrameType::Ping => {
                self.sender.send_pong(transport, frame.sequence)?;
            }

            FrameType::Pong => {
                self.sender.process_selective_ack(frame.sequence, timestamp);
            }

            FrameType::Fin => {
                self.state = ConnectionState::Closing;
                // Send FIN-ACK
                let ack_frame = Frame::new_ack(frame.sequence.wrapping_add(1));
                let mut buf = [0u8; FRAME_HEADER_SIZE];
                let size = ack_frame.serialize(&mut buf)?;
                transport.write_all(&buf[..size])?;
            }

            FrameType::FinAck => {
                self.state = ConnectionState::Closed;
            }

            FrameType::Reset => {
                self.state = ConnectionState::Closed;
            }

            FrameType::WindowUpdate => {
                // Peer is updating their receive window
                // Could be used for flow control adjustment
            }
        }

        Ok(())
    }

    /// Performs periodic maintenance tasks.
    ///
    /// This should be called regularly to:
    /// - Check for retransmissions
    /// - Clean up timed out reassembly entries
    /// - Send keep-alive pings
    pub fn poll<T: Transport>(&mut self, transport: &mut T, timestamp: u64) -> Result<()> {
        // Process incoming data
        self.process_incoming(transport, timestamp)?;

        // Check for retransmissions
        self.sender.check_retransmit(transport, timestamp)?;

        // Clean up receiver
        self.receiver.cleanup(timestamp);

        Ok(())
    }

    /// Initiates graceful connection close.
    pub fn close<T: Transport>(&mut self, transport: &mut T) -> Result<()> {
        if self.state != ConnectionState::Connected {
            return Ok(());
        }

        self.sender.close(transport)?;
        self.state = ConnectionState::Closing;

        Ok(())
    }

    /// Resets the protocol to initial state.
    pub fn reset(&mut self) {
        self.sender.reset();
        self.receiver.reset();
        self.state = ConnectionState::Idle;
        self.recv_pos = 0;
        self.recv_len = 0;
        self.stats = ProtocolStats::default();
    }

    /// Returns true if there's data available to send.
    pub fn can_send(&self) -> bool {
        self.state == ConnectionState::Connected && self.sender.can_send()
    }

    /// Returns true if there's data available to read.
    pub fn has_data(&self) -> bool {
        self.receiver.has_data()
    }

    /// Sends a ping and returns the sequence number.
    pub fn ping<T: Transport>(&mut self, transport: &mut T, timestamp: u64) -> Result<u32> {
        if self.state != ConnectionState::Connected {
            return Err(Error::InvalidState);
        }

        self.sender.send_ping(transport, timestamp)
    }
}

/// Builder for creating Protocol instances with custom configuration.
pub struct ProtocolBuilder {
    config: Config,
}

impl ProtocolBuilder {
    /// Creates a new builder with default configuration.
    pub fn new() -> Self {
        Self {
            config: Config::default(),
        }
    }

    /// Sets the maximum frame size (including header, must be > 24).
    pub fn max_frame_size(mut self, size: usize) -> Self {
        self.config = self.config.with_max_frame_size(size);
        self
    }

    /// Sets the window size.
    pub fn window_size(mut self, size: u16) -> Self {
        self.config.window_size = size;
        self
    }

    /// Sets the retransmit timeout.
    pub fn retransmit_timeout_ms(mut self, ms: u32) -> Self {
        self.config.retransmit_timeout_ms = ms;
        self
    }

    /// Sets the maximum retransmit attempts.
    pub fn max_retransmit(mut self, count: u8) -> Self {
        self.config.max_retransmit = count;
        self
    }

    /// Enables or disables checksum.
    pub fn checksum(mut self, enable: bool) -> Self {
        self.config.enable_checksum = enable;
        self
    }

    /// Builds the Protocol instance.
    pub fn build(self) -> Protocol {
        Protocol::new(self.config)
    }
}

impl Default for ProtocolBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_creation() {
        let protocol = Protocol::with_defaults();
        assert_eq!(protocol.state(), ConnectionState::Idle);
    }

    #[test]
    fn test_protocol_builder() {
        let protocol = ProtocolBuilder::new()
            .max_frame_size(536)  // 512 payload + 24 header
            .window_size(32)
            .retransmit_timeout_ms(500)
            .build();

        assert_eq!(protocol.config().max_frame_size, 536);
        assert_eq!(protocol.config().window_size, 32);
    }
}
