//! Custom transport implementation example.
//!
//! This example demonstrates how to implement a custom transport
//! that can be used with XTransport protocol.
//!
//! Run with: cargo run --example custom_transport --features std

use xtransport::transport::Transport;
use xtransport::error::{Error, Result};
use xtransport::{Crc32, Frame};

/// A simulated unreliable transport that randomly drops or corrupts packets.
/// 
/// This demonstrates how XTransport handles packet loss through retransmission.
pub struct UnreliableTransport {
    /// Internal buffer for data in transit.
    buffer: Vec<u8>,
    
    /// Read position in buffer.
    read_pos: usize,
    
    /// Packet loss probability (0.0 - 1.0).
    loss_rate: f32,
    
    /// Corruption probability (0.0 - 1.0).
    corruption_rate: f32,
    
    /// Simple counter for pseudo-random behavior.
    counter: u32,
    
    /// Statistics.
    packets_dropped: usize,
    packets_corrupted: usize,
    packets_delivered: usize,
}

impl UnreliableTransport {
    /// Creates a new unreliable transport with given loss and corruption rates.
    pub fn new(loss_rate: f32, corruption_rate: f32) -> Self {
        Self {
            buffer: Vec::with_capacity(8192),
            read_pos: 0,
            loss_rate,
            corruption_rate,
            counter: 0,
            packets_dropped: 0,
            packets_corrupted: 0,
            packets_delivered: 0,
        }
    }

    /// Simple pseudo-random number generator (0.0 - 1.0).
    fn random(&mut self) -> f32 {
        self.counter = self.counter.wrapping_mul(1103515245).wrapping_add(12345);
        ((self.counter >> 16) & 0x7fff) as f32 / 32767.0
    }

    /// Returns statistics about packet delivery.
    pub fn stats(&self) -> (usize, usize, usize) {
        (self.packets_dropped, self.packets_corrupted, self.packets_delivered)
    }
}

impl Transport for UnreliableTransport {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if self.read_pos >= self.buffer.len() {
            return Err(Error::WouldBlock);
        }

        let available = self.buffer.len() - self.read_pos;
        let to_read = core::cmp::min(buf.len(), available);

        buf[..to_read].copy_from_slice(&self.buffer[self.read_pos..self.read_pos + to_read]);
        self.read_pos += to_read;

        // Reset buffer if fully consumed
        if self.read_pos >= self.buffer.len() {
            self.buffer.clear();
            self.read_pos = 0;
        }

        Ok(to_read)
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        // Simulate packet loss
        if self.random() < self.loss_rate {
            self.packets_dropped += 1;
            // Silently "drop" the packet by not storing it
            return Ok(buf.len());
        }

        // Simulate corruption
        let mut data = buf.to_vec();
        if self.random() < self.corruption_rate {
            self.packets_corrupted += 1;
            // Flip some bits
            if !data.is_empty() {
                let len = data.len();
                let pos = (self.random() * len as f32) as usize;
                data[pos.min(len - 1)] ^= 0xFF;
            }
        }

        self.buffer.extend_from_slice(&data);
        self.packets_delivered += 1;

        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

/// A bandwidth-limited transport.
pub struct BandwidthLimitedTransport<T> {
    inner: T,
    bytes_per_second: usize,
    bytes_sent_this_second: usize,
    last_reset: u64,
    current_time: u64,
}

impl<T: Transport> BandwidthLimitedTransport<T> {
    /// Creates a new bandwidth-limited transport.
    pub fn new(inner: T, bytes_per_second: usize) -> Self {
        Self {
            inner,
            bytes_per_second,
            bytes_sent_this_second: 0,
            last_reset: 0,
            current_time: 0,
        }
    }

    /// Advances the simulated time.
    pub fn advance_time(&mut self, delta_ms: u64) {
        self.current_time += delta_ms;

        // Reset byte counter every second
        if self.current_time - self.last_reset >= 1000 {
            self.bytes_sent_this_second = 0;
            self.last_reset = self.current_time;
        }
    }
}

impl<T: Transport> Transport for BandwidthLimitedTransport<T> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.inner.read(buf)
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        // Check bandwidth limit
        let available = self.bytes_per_second.saturating_sub(self.bytes_sent_this_second);
        if available == 0 {
            return Err(Error::WouldBlock);
        }

        let to_send = core::cmp::min(buf.len(), available);
        let result = self.inner.write(&buf[..to_send])?;
        self.bytes_sent_this_second += result;

        Ok(result)
    }

    fn flush(&mut self) -> Result<()> {
        self.inner.flush()
    }
}

fn main() {
    println!("=== XTransport Custom Transport Example ===\n");

    // Example 1: Unreliable transport
    println!("1. Unreliable Transport (30% loss, 10% corruption):");
    let mut transport = UnreliableTransport::new(0.3, 0.1);

    // Seed the random generator
    transport.counter = 12345;

    // Send multiple packets
    for i in 0..10 {
        let data = format!("Packet {}", i);
        let _ = transport.write(data.as_bytes());
    }

    let (dropped, corrupted, delivered) = transport.stats();
    println!("   Packets sent: 10");
    println!("   Packets dropped: {}", dropped);
    println!("   Packets corrupted: {}", corrupted);
    println!("   Packets delivered: {}\n", delivered);

    // Example 2: Frame integrity check
    println!("2. Frame Integrity with CRC32:");

    // Create a valid frame
    let frame = Frame::new_data(1, 0, 1, 0, 1, b"Test data");
    let mut buf = [0u8; 256];
    let size = frame.serialize(&mut buf).unwrap();

    println!("   Original frame size: {} bytes", size);

    // Verify it decodes correctly
    match Frame::deserialize(&buf[..size]) {
        Ok((f, _)) => println!("   Original: Valid frame, seq={}", f.sequence),
        Err(e) => println!("   Original: Error {:?}", e),
    }

    // Corrupt the frame
    buf[size - 1] ^= 0xFF;
    match Frame::deserialize(&buf[..size]) {
        Ok((f, _)) => println!("   Corrupted: Unexpectedly valid, seq={}", f.sequence),
        Err(e) => println!("   Corrupted: Detected error - {:?}", e),
    }

    // Example 3: CRC32 performance
    println!("\n3. CRC32 Performance:");
    let large_data = vec![0xABu8; 1_000_000];

    let start = std::time::Instant::now();
    let iterations = 100;

    for _ in 0..iterations {
        let _ = Crc32::compute(&large_data);
    }

    let elapsed = start.elapsed();
    let throughput_mbps = (large_data.len() * iterations) as f64 
        / elapsed.as_secs_f64() 
        / 1_000_000.0;

    println!("   Data size: 1 MB");
    println!("   Iterations: {}", iterations);
    println!("   Time: {:?}", elapsed);
    println!("   Throughput: {:.2} MB/s\n", throughput_mbps);

    // Example 4: Loopback transport
    println!("4. Loopback Transport:");
    
    use xtransport::transport::LoopbackTransport;
    let mut loopback: LoopbackTransport<1024> = LoopbackTransport::new();

    // Demonstrate write and read
    let data = b"Hello, World!";
    loopback.write(data).expect("write failed");
    println!("   Wrote: {} bytes", data.len());

    let mut read_buf = [0u8; 32];
    let n = loopback.read(&mut read_buf).expect("read failed");
    println!("   Read: {:?}", std::str::from_utf8(&read_buf[..n]).unwrap());

    // Example 5: Bandwidth limited transport
    println!("\n5. Bandwidth Limited Transport:");

    let inner: LoopbackTransport<4096> = LoopbackTransport::new();
    let mut limited = BandwidthLimitedTransport::new(inner, 100); // 100 bytes/sec

    let data = b"Hello";
    let result1 = limited.write(data);
    println!("   First write (5 bytes): {:?}", result1);

    let large_data = [0u8; 200];
    let result2 = limited.write(&large_data);
    println!("   Second write (200 bytes, limit 100): {:?}", result2);

    // Advance time to reset bandwidth
    limited.advance_time(1000);
    let result3 = limited.write(&large_data);
    println!("   After 1 second (200 bytes): {:?}", result3);

    println!("\n=== Custom Transport Example Complete ===");
}
