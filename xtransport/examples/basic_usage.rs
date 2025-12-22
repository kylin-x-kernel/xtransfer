//! Basic usage example demonstrating the XTransport protocol.
//!
//! This example shows how to:
//! - Create a protocol instance
//! - Use the loopback transport for testing
//! - Send and receive data
//!
//! Run with: cargo run --example basic_usage --features std

use xtransport::{Config, Crc32, Frame, Packet, ProtocolBuilder, FRAME_HEADER_SIZE};
use xtransport::transport::LoopbackTransport;

fn main() {
    println!("=== XTransport Basic Usage Example ===\n");

    // Example 1: CRC32 checksum
    println!("1. CRC32 Checksum:");
    let data = b"Hello, XTransport!";
    let checksum = Crc32::compute(data);
    println!("   Data: {:?}", core::str::from_utf8(data).unwrap());
    println!("   CRC32: 0x{:08X}", checksum);
    println!("   Verify: {}\n", Crc32::verify(data, checksum));

    // Example 2: Frame serialization
    println!("2. Frame Serialization:");
    let payload = b"Test payload data";
    let frame = Frame::new_data(
        1,              // sequence
        0,              // ack
        100,            // packet_id
        0,              // fragment_index
        1,              // total_fragments
        payload,
    );

    let mut buf = [0u8; 256];
    let size = frame.serialize(&mut buf).expect("serialize failed");
    println!("   Payload: {:?}", core::str::from_utf8(payload).unwrap());
    println!("   Frame size: {} bytes (header: {}, payload: {})", 
             size, FRAME_HEADER_SIZE, payload.len());

    // Deserialize and verify
    let (decoded, _) = Frame::deserialize(&buf[..size]).expect("deserialize failed");
    println!("   Deserialized sequence: {}", decoded.sequence);
    println!("   Deserialized packet_id: {}", decoded.packet_id);
    println!("   Payload match: {}\n", decoded.payload == payload);

    // Example 3: Protocol configuration
    println!("3. Protocol Configuration:");
    let config = Config::default();
    println!("   Default config:");
    println!("     - Max frame size: {} bytes", config.max_frame_size);
    println!("     - Max payload size: {} bytes", config.max_payload_size());
    println!("     - Window size: {}", config.window_size);
    println!("     - Retransmit timeout: {}ms", config.retransmit_timeout_ms);
    println!("     - Max retransmit: {}", config.max_retransmit);

    let low_latency = Config::low_latency();
    println!("   Low latency config:");
    println!("     - Max frame size: {} bytes", low_latency.max_frame_size);
    println!("     - Max payload size: {} bytes", low_latency.max_payload_size());
    println!("     - Window size: {}", low_latency.window_size);
    println!("     - Retransmit timeout: {}ms\n", low_latency.retransmit_timeout_ms);

    // Example 4: Protocol builder pattern
    println!("4. Protocol Builder:");
    let protocol = ProtocolBuilder::new()
        .max_frame_size(536)  // 512 payload + 24 header
        .window_size(32)
        .retransmit_timeout_ms(500)
        .max_retransmit(3)
        .build();

    println!("   Custom protocol created:");
    println!("     - Max frame size: {}", protocol.config().max_frame_size);
    println!("     - Max payload size: {}", protocol.config().max_payload_size());
    println!("     - Window size: {}", protocol.config().window_size);
    println!("     - State: {:?}\n", protocol.state());

    // Example 5: Loopback transport
    println!("5. Loopback Transport:");
    use xtransport::transport::Transport;

    let mut transport: LoopbackTransport<1024> = LoopbackTransport::new();
    
    // Write some data
    let write_data = b"Hello via loopback!";
    transport.write(write_data).expect("write failed");
    println!("   Written: {:?}", core::str::from_utf8(write_data).unwrap());
    println!("   Available: {} bytes", transport.available());

    // Read it back
    let mut read_buf = [0u8; 64];
    let n = transport.read(&mut read_buf).expect("read failed");
    println!("   Read: {:?}", core::str::from_utf8(&read_buf[..n]).unwrap());
    println!("   Match: {}\n", &read_buf[..n] == write_data);

    // Example 6: Multiple fragments
    println!("6. Packet Fragmentation:");

    let large_data: Vec<u8> = (0..2500).map(|i| (i % 256) as u8).collect();
    let packet = Packet::new(1, &large_data).expect("packet creation failed");
    
    let fragment_count_1k = packet.fragment_count(1024);
    let fragment_count_512 = packet.fragment_count(512);
    
    println!("   Data size: {} bytes", large_data.len());
    println!("   Fragments (1024 byte max): {}", fragment_count_1k);
    println!("   Fragments (512 byte max): {}", fragment_count_512);

    // Show fragment sizes
    println!("   Fragment breakdown (1024 byte max):");
    for i in 0..fragment_count_1k {
        if let Some(frag) = packet.fragment_data(i, 1024) {
            println!("     Fragment {}: {} bytes", i, frag.len());
        }
    }

    println!("\n=== Example Complete ===");
}
