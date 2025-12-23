use crate::{
    error::{Error, ErrorKind},
    io::{Read, Write},
    protocol::{Packet, PacketHeader, PacketType, MessageHead, HEADER_SIZE, MAX_PAYLOAD_SIZE, MESSAGE_HEAD_SIZE},
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
}

impl<T: Read + Write> XTransport<T> {
    pub fn new(inner: T) -> Self {
        XTransport {
            inner,
            send_seq: 0,
            recv_seq: 0,
            next_message_id: 1,
            recv_buffer: Vec::new(),
            recv_pos: 0,
            recv_available: 0,
        }
    }

    fn send_packet(&mut self, pkt_type: PacketType, data: &[u8]) -> Result<()> {
        let packet = Packet::new(pkt_type, self.send_seq, data.to_vec());
        self.send_seq = self.send_seq.wrapping_add(1);

        // Send header
        let header_bytes = packet.header.to_bytes();
        self.inner.write_all(&header_bytes)?;

        // Send data
        self.inner.write_all(&packet.data)?;
        
        log::trace!("Sent packet type={:?}, seq={}, len={}", pkt_type, packet.header.seq, packet.data.len());
        
        Ok(())
    }

    fn recv_packet(&mut self) -> Result<Packet> {
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

        // Update receive sequence
        self.recv_seq = packet.header.seq.wrapping_add(1);

        Ok(packet)
    }

    /// Send a complete message (automatically handles fragmentation)
    pub fn send_message(&mut self, data: &[u8]) -> Result<()> {
        if data.len() <= MAX_PAYLOAD_SIZE {
            // Small message: single Data packet
            self.send_packet(PacketType::Data, data)?;
            log::debug!("Sent single-packet message: {} bytes", data.len());
        } else {
            // Large message: MessageHead + multiple MessageData packets
            let message_id = self.next_message_id;
            self.next_message_id = self.next_message_id.wrapping_add(1);
            
            let packet_count = ((data.len() + MAX_PAYLOAD_SIZE - 1) / MAX_PAYLOAD_SIZE) as u32;
            
            // Send MessageHead
            let head = MessageHead::new(data.len() as u64, message_id, packet_count);
            self.send_packet(PacketType::MessageHead, &head.to_bytes())?;
            
            log::debug!("Sending large message: id={}, total={} bytes, packets={}", 
                       message_id, data.len(), packet_count);
            
            // Send MessageData packets
            for chunk in data.chunks(MAX_PAYLOAD_SIZE) {
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
            PacketType::MessageData => {
                // Unexpected: should not receive MessageData as first packet
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

        // Send first chunk (up to MAX_PAYLOAD_SIZE)
        let to_send = core::cmp::min(buf.len(), MAX_PAYLOAD_SIZE);
        self.send_packet(PacketType::Data, &buf[..to_send])?;

        Ok(to_send)
    }

    fn flush(&mut self) -> Result<()> {
        self.inner.flush()
    }
}
