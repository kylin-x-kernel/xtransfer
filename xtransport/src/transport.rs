use crate::{
    config::{TransportConfig, HEADER_SIZE, MESSAGE_HEAD_SIZE},
    error::{Error, ErrorKind},
    io::{Read, Write},
    protocol::{Packet, PacketHeader, PacketType, MessageHead},
    Result,
};
use alloc::vec::Vec;

pub struct XTransport<T> {
    inner: T,
    send_seq: u32,
    recv_seq: u32,
    next_message_id: u64,
    recv_buffer: Vec<u8>,
    recv_pos: usize,
    recv_available: usize,
    config: TransportConfig,
}

impl<T: Read + Write> XTransport<T> {
    pub fn new(inner: T, config: TransportConfig) -> Self {
        XTransport {
            inner,
            send_seq: 0,
            recv_seq: 0,
            next_message_id: 1,
            recv_buffer: Vec::new(),
            recv_pos: 0,
            recv_available: 0,
            config,
        }
    }

    fn send_packet(&mut self, pkt_type: PacketType, data: &[u8]) -> Result<()> {
        let packet = Packet::new(pkt_type, self.send_seq, data.to_vec());
        let seq = packet.header.seq;
        self.send_seq = self.send_seq.wrapping_add(1);

        // Combine header and data into a single buffer for atomic send
        let header_bytes = packet.header.to_bytes();
        let mut combined = Vec::with_capacity(header_bytes.len() + packet.data.len());
        combined.extend_from_slice(&header_bytes);
        combined.extend_from_slice(&packet.data);
        
        // Send combined buffer in one write call
        self.inner.write_all(&combined)?;
        
        log::trace!("Sent packet type={:?}, seq={}, len={}", pkt_type, seq, packet.data.len());
        
        // Wait for ACK if configured and not sending an ACK itself
        if self.config.wait_for_ack && pkt_type != PacketType::Ack {
            let ack_packet = self.recv_packet_internal()?;
            if ack_packet.header.pkt_type != PacketType::Ack as u8 {
                return Err(Error::new(ErrorKind::InvalidPacket));
            }
            if ack_packet.data.len() < 4 {
                return Err(Error::new(ErrorKind::InvalidPacket));
            }
            let ack_seq = u32::from_le_bytes([ack_packet.data[0], ack_packet.data[1], ack_packet.data[2], ack_packet.data[3]]);
            if ack_seq != seq {
                log::warn!("ACK seq mismatch: expected {}, got {}", seq, ack_seq);
                return Err(Error::new(ErrorKind::InvalidPacket));
            }
            log::trace!("Received ACK for seq={}", seq);
        }
        
        Ok(())
    }

    fn send_ack(&mut self, seq: u32) -> Result<()> {
        let ack_data = seq.to_le_bytes();
        let ack_packet = Packet::new(PacketType::Ack, self.send_seq, ack_data.to_vec());
        self.send_seq = self.send_seq.wrapping_add(1);
        
        let header_bytes = ack_packet.header.to_bytes();
        let mut combined = Vec::with_capacity(header_bytes.len() + ack_packet.data.len());
        combined.extend_from_slice(&header_bytes);
        combined.extend_from_slice(&ack_packet.data);
        self.inner.write_all(&combined)?;
        
        log::trace!("Sent ACK for seq={}", seq);
        Ok(())
    }

    fn recv_packet_internal(&mut self) -> Result<Packet> {
        // Read header
        let mut header_buf = [0u8; HEADER_SIZE];
        self.inner.read_exact(&mut header_buf)?;
        let header = PacketHeader::from_bytes(&header_buf)?;

        // Read data
        let mut data = alloc::vec![0u8; header.length as usize];
        self.inner.read_exact(&mut data)?;

        let packet = Packet { header, data };

        // Verify CRC
        if !packet.verify_crc() {
            return Err(Error::new(ErrorKind::CrcMismatch));
        }

        log::trace!("Received packet seq={}, len={}", packet.header.seq, packet.data.len());

        Ok(packet)
    }

    fn recv_packet(&mut self) -> Result<Packet> {
        let packet = self.recv_packet_internal()?;
        
        // Send ACK if configured and not receiving an ACK itself
        let pkt_type = PacketType::from_u8(packet.header.pkt_type)
            .ok_or_else(|| Error::new(ErrorKind::InvalidPacket))?;
        
        if self.config.wait_for_ack && pkt_type != PacketType::Ack {
            self.send_ack(packet.header.seq)?;
        }
        
        // Update receive sequence
        self.recv_seq = packet.header.seq.wrapping_add(1);

        Ok(packet)
    }

    /// Send a complete message (automatically handles fragmentation)
    pub fn send_message(&mut self, data: &[u8]) -> Result<()> {
        if data.len() <= self.config.max_payload_size {
            // Small message: single Data packet
            self.send_packet(PacketType::Data, data)?;
            log::debug!("Sent single-packet message: {} bytes", data.len());
        } else {
            // Large message: MessageHead + multiple MessageData packets
            let message_id = self.next_message_id;
            self.next_message_id = self.next_message_id.wrapping_add(1);
            
            let packet_count = ((data.len() + self.config.max_payload_size - 1) / self.config.max_payload_size) as u32;
            
            // Send MessageHead
            let head = MessageHead::new(data.len() as u64, message_id, packet_count);
            self.send_packet(PacketType::MessageHead, &head.to_bytes())?;
            
            log::debug!("Sending large message: id={}, total={} bytes, packets={}", 
                       message_id, data.len(), packet_count);
            
            // Send MessageData packets
            for chunk in data.chunks(self.config.max_payload_size) {
                self.send_packet(PacketType::MessageData, chunk)?;
            }
            
            log::debug!("Large message sent: id={}", message_id);
        }
        
        self.inner.flush()?;
        Ok(())
    }

    /// Receive a complete message (automatically handles reassembly)
    pub fn recv_message(&mut self) -> Result<Vec<u8>> {
        // Read first packet to determine type
        let mut header_buf = [0u8; HEADER_SIZE];
        self.inner.read_exact(&mut header_buf)?;
        let header = PacketHeader::from_bytes(&header_buf)?;
        
        let pkt_type = PacketType::from_u8(header.pkt_type)
            .ok_or_else(|| Error::new(ErrorKind::InvalidPacket))?;
        
        match pkt_type {
            PacketType::Data => {
                // Single packet message
                let mut data = alloc::vec![0u8; header.length as usize];
                self.inner.read_exact(&mut data)?;
                
                let packet = Packet { header, data };
                if !packet.verify_crc() {
                    return Err(Error::new(ErrorKind::CrcMismatch));
                }
                
                // Send ACK if configured
                if self.config.wait_for_ack {
                    self.send_ack(packet.header.seq)?;
                }
                
                log::debug!("Received single-packet message: {} bytes", packet.data.len());
                Ok(packet.data)
            }
            PacketType::MessageHead => {
                // Multi-packet message
                let mut head_data = alloc::vec![0u8; header.length as usize];
                self.inner.read_exact(&mut head_data)?;
                
                let packet = Packet { header, data: head_data };
                if !packet.verify_crc() {
                    return Err(Error::new(ErrorKind::CrcMismatch));
                }
                
                // Send ACK for MessageHead if configured
                if self.config.wait_for_ack {
                    self.send_ack(packet.header.seq)?;
                }
                
                if packet.data.len() < MESSAGE_HEAD_SIZE {
                    return Err(Error::new(ErrorKind::InvalidPacket));
                }
                
                let mut head_bytes = [0u8; MESSAGE_HEAD_SIZE];
                head_bytes.copy_from_slice(&packet.data[..MESSAGE_HEAD_SIZE]);
                let msg_head = MessageHead::from_bytes(&head_bytes)?;
                
                log::debug!("Receiving large message: id={}, total={} bytes, packets={}", 
                           msg_head.message_id, msg_head.total_length, msg_head.packet_count);
                
                // Receive all data packets
                let mut result = alloc::vec![0u8; msg_head.total_length as usize];
                let mut offset = 0;
                
                for i in 0..msg_head.packet_count {
                    let mut data_header_buf = [0u8; HEADER_SIZE];
                    self.inner.read_exact(&mut data_header_buf)?;
                    let data_header = PacketHeader::from_bytes(&data_header_buf)?;
                    
                    let data_type = PacketType::from_u8(data_header.pkt_type)
                        .ok_or_else(|| Error::new(ErrorKind::InvalidPacket))?;
                    
                    if data_type != PacketType::MessageData {
                        return Err(Error::new(ErrorKind::InvalidPacket));
                    }
                    
                    let mut chunk = alloc::vec![0u8; data_header.length as usize];
                    self.inner.read_exact(&mut chunk)?;
                    
                    let data_packet = Packet { header: data_header, data: chunk };
                    if !data_packet.verify_crc() {
                        return Err(Error::new(ErrorKind::CrcMismatch));
                    }
                    
                    // Send ACK for each MessageData if configured
                    if self.config.wait_for_ack {
                        self.send_ack(data_packet.header.seq)?;
                    }
                    
                    let to_copy = core::cmp::min(data_packet.data.len(), result.len() - offset);
                    result[offset..offset + to_copy].copy_from_slice(&data_packet.data[..to_copy]);
                    offset += to_copy;
                    
                    if (i + 1) % 100 == 0 || i + 1 == msg_head.packet_count {
                        log::debug!("Progress: {}/{} packets received", i + 1, msg_head.packet_count);
                    }
                }
                
                log::debug!("Large message received: id={}, {} bytes", msg_head.message_id, result.len());
                Ok(result)
            }
            PacketType::MessageData | PacketType::Ack => {
                // Unexpected: should not receive MessageData or Ack as first packet
                Err(Error::new(ErrorKind::InvalidPacket))
            }
        }
    }
}

impl<T: Read + Write> Read for XTransport<T> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if self.recv_pos >= self.recv_available {
            // Need to receive a new packet
            let packet = self.recv_packet()?;
            self.recv_buffer = packet.data;
            self.recv_pos = 0;
            self.recv_available = self.recv_buffer.len();
        }

        // Copy data from receive buffer
        let to_copy = core::cmp::min(buf.len(), self.recv_available - self.recv_pos);
        buf[..to_copy].copy_from_slice(&self.recv_buffer[self.recv_pos..self.recv_pos + to_copy]);
        self.recv_pos += to_copy;

        Ok(to_copy)
    }
}

impl<T: Read + Write> Write for XTransport<T> {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        // Send first chunk (up to max_payload_size)
        let to_send = core::cmp::min(buf.len(), self.config.max_payload_size);
        self.send_packet(PacketType::Data, &buf[..to_send])?;

        Ok(to_send)
    }

    fn flush(&mut self) -> Result<()> {
        self.inner.flush()
    }
}
