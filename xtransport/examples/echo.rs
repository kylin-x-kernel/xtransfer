//! Echo server and client example using XTransport.
//!
//! This example demonstrates a simple echo protocol where:
//! - Server echoes back whatever data it receives
//! - Client sends messages and verifies the echo
//!
//! Run with: cargo run --example echo --features std

use xtransport::{Config, Protocol};
use xtransport::transport::{Transport, LoopbackTransport};
use xtransport::error::Result;

/// Simulates a pair of connected transports.
/// 
/// In a real application, these would be actual network sockets.
struct TransportPair {
    client_to_server: LoopbackTransport<8192>,
    server_to_client: LoopbackTransport<8192>,
}

impl TransportPair {
    fn new() -> Self {
        Self {
            client_to_server: LoopbackTransport::new(),
            server_to_client: LoopbackTransport::new(),
        }
    }
}

/// Client-side transport wrapper.
struct ClientTransport<'a> {
    pair: &'a mut TransportPair,
}

impl<'a> Transport for ClientTransport<'a> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.pair.server_to_client.read(buf)
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.pair.client_to_server.write(buf)
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

/// Server-side transport wrapper.
struct ServerTransport<'a> {
    pair: &'a mut TransportPair,
}

impl<'a> Transport for ServerTransport<'a> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.pair.client_to_server.read(buf)
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.pair.server_to_client.write(buf)
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

fn main() {
    println!("=== XTransport Echo Example ===\n");

    // Create transport pair
    let mut pair = TransportPair::new();

    // Create client and server protocols
    let config = Config::default()
        .with_max_frame_size(280)  // 256 payload + 24 header
        .with_window_size(16);

    let mut client = Protocol::new(config);
    let mut server = Protocol::new(config);

    // Simulate timestamp
    let mut timestamp = 0u64;

    // --- Connection Handshake ---
    println!("1. Connection Handshake:");

    // Client initiates connection
    {
        let mut client_transport = ClientTransport { pair: &mut pair };
        client.connect(&mut client_transport, timestamp).expect("connect failed");
    }
    println!("   Client: SYNC sent");
    timestamp += 10;

    // Server receives SYNC and accepts
    {
        let mut server_transport = ServerTransport { pair: &mut pair };
        server.process_incoming(&mut server_transport, timestamp).expect("process failed");
        server.accept(&mut server_transport, timestamp).expect("accept failed");
    }
    println!("   Server: SYNC received, SYNC-ACK sent");
    timestamp += 10;

    // Client completes handshake
    {
        let mut client_transport = ClientTransport { pair: &mut pair };
        client.process_incoming(&mut client_transport, timestamp).expect("process failed");
    }
    println!("   Client: Connection established");
    println!("   Client state: {:?}", client.state());
    println!("   Server state: {:?}\n", server.state());

    // --- Echo Test ---
    println!("2. Echo Test:");

    let messages = [
        "Hello, Server!",
        "This is a test message.",
        "XTransport is working!",
        "Final message.",
    ];

    for (i, msg) in messages.iter().enumerate() {
        timestamp += 100;

        // Client sends message
        {
            let mut client_transport = ClientTransport { pair: &mut pair };
            client.send(&mut client_transport, msg.as_bytes(), timestamp)
                .expect("send failed");
        }
        println!("   [{}] Client sent: {}", i + 1, msg);

        // Server receives and echoes
        let mut recv_buf = [0u8; 1024];
        {
            let mut server_transport = ServerTransport { pair: &mut pair };
            server.process_incoming(&mut server_transport, timestamp).expect("process failed");

            if server.has_data() {
                match server.recv(&mut server_transport, &mut recv_buf, timestamp) {
                    Ok(n) => {
                        let received = core::str::from_utf8(&recv_buf[..n]).unwrap_or("<invalid>");
                        println!("   [{}] Server received: {}", i + 1, received);

                        // Echo back
                        server.send(&mut server_transport, &recv_buf[..n], timestamp)
                            .expect("echo send failed");
                        println!("   [{}] Server echoed back", i + 1);
                    }
                    Err(e) => println!("   [{}] Server recv error: {:?}", i + 1, e),
                }
            }
        }

        timestamp += 50;

        // Client receives echo
        {
            let mut client_transport = ClientTransport { pair: &mut pair };
            client.process_incoming(&mut client_transport, timestamp).expect("process failed");

            if client.has_data() {
                match client.recv(&mut client_transport, &mut recv_buf, timestamp) {
                    Ok(n) => {
                        let echo = core::str::from_utf8(&recv_buf[..n]).unwrap_or("<invalid>");
                        let matches = echo == *msg;
                        println!("   [{}] Client received echo: {} (match: {})", i + 1, echo, matches);
                    }
                    Err(e) => println!("   [{}] Client recv error: {:?}", i + 1, e),
                }
            }
        }
        println!();
    }

    // --- Statistics ---
    println!("3. Statistics:");
    let client_stats = client.stats();
    let server_stats = server.stats();

    println!("   Client:");
    println!("     - Packets sent: {}", client_stats.packets_sent);
    println!("     - Packets received: {}", client_stats.packets_received);
    println!("     - Bytes sent: {}", client_stats.bytes_sent);
    println!("     - Bytes received: {}", client_stats.bytes_received);

    println!("   Server:");
    println!("     - Packets sent: {}", server_stats.packets_sent);
    println!("     - Packets received: {}", server_stats.packets_received);
    println!("     - Bytes sent: {}", server_stats.bytes_sent);
    println!("     - Bytes received: {}", server_stats.bytes_received);

    println!("\n=== Echo Example Complete ===");
}
