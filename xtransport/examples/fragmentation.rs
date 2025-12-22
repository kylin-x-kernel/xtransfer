//! Fragmentation and reassembly example.
//!
//! This example demonstrates how XTransport handles large messages
//! by fragmenting them into smaller frames and reassembling them.
//!
//! Run with: cargo run --example fragmentation --features std

use xtransport::{Crc32, Frame, Packet};
use xtransport::reliable::Reassembler;

fn main() {
    println!("=== XTransport Fragmentation Example ===\n");

    // Configuration
    let max_fragment_size = 100; // Small for demonstration
    let packet_id = 1u16;

    // Create large data
    let original_data: Vec<u8> = (0..350).map(|i| (i % 256) as u8).collect();
    println!("1. Original Data:");
    println!("   Size: {} bytes", original_data.len());
    println!("   CRC32: 0x{:08X}\n", Crc32::compute(&original_data));

    // Create packet and determine fragments
    let packet = Packet::new(packet_id, &original_data).expect("packet creation failed");
    let fragment_count = packet.fragment_count(max_fragment_size);

    println!("2. Fragmentation:");
    println!("   Max fragment size: {} bytes", max_fragment_size);
    println!("   Fragment count: {}\n", fragment_count);

    // Create frames for each fragment
    let mut frames = Vec::new();
    for i in 0..fragment_count {
        let fragment_data = packet.fragment_data(i, max_fragment_size).unwrap();
        let frame = Frame::new_data(
            i as u32,           // sequence
            0,                   // ack
            packet_id,
            i as u8,
            fragment_count as u8,
            fragment_data,
        );

        println!("   Fragment {}: {} bytes (seq={}, is_first={}, is_last={})",
                 i, fragment_data.len(), frame.sequence,
                 frame.is_first_fragment(), frame.is_last_fragment());

        frames.push(frame);
    }

    // Serialize all frames
    println!("\n3. Serialization:");
    let mut serialized_frames = Vec::new();
    let mut total_wire_size = 0;

    for (i, frame) in frames.iter().enumerate() {
        let mut buf = vec![0u8; frame.wire_size()];
        let size = frame.serialize(&mut buf).unwrap();
        total_wire_size += size;
        println!("   Frame {}: {} bytes on wire", i, size);
        serialized_frames.push(buf);
    }

    let overhead = total_wire_size - original_data.len();
    let overhead_percent = (overhead as f64 / original_data.len() as f64) * 100.0;
    println!("   Total wire size: {} bytes", total_wire_size);
    println!("   Overhead: {} bytes ({:.1}%)\n", overhead, overhead_percent);

    // Reassembly - simulate out-of-order delivery
    println!("4. Reassembly (out-of-order):");

    // Create reassembler
    let mut reassembler: Reassembler<4, 4096> = Reassembler::new(max_fragment_size, 5000);

    // Deliver frames in scrambled order: 2, 0, 3, 1
    let delivery_order = [2, 0, 3, 1];
    let timestamp = 0;

    for &idx in &delivery_order {
        if idx >= serialized_frames.len() {
            continue;
        }

        // Deserialize frame
        let (frame, _) = Frame::deserialize(&serialized_frames[idx]).unwrap();
        println!("   Received fragment {} (seq={})", frame.fragment_index, frame.sequence);

        // Process frame
        match reassembler.process_frame(&frame, timestamp) {
            Ok(Some(pid)) => println!("   -> Packet {} complete!", pid),
            Ok(None) => println!("   -> Waiting for more fragments..."),
            Err(e) => println!("   -> Error: {:?}", e),
        }
    }

    // Get reassembled data
    println!("\n5. Verification:");
    let mut reassembled = vec![0u8; 4096];
    match reassembler.take_completed(packet_id, &mut reassembled) {
        Ok(len) => {
            reassembled.truncate(len);
            println!("   Reassembled size: {} bytes", len);
            println!("   Reassembled CRC32: 0x{:08X}", Crc32::compute(&reassembled));

            let matches = reassembled == original_data;
            println!("   Data matches original: {}", matches);

            if !matches {
                // Find first difference
                for (i, (a, b)) in original_data.iter().zip(reassembled.iter()).enumerate() {
                    if a != b {
                        println!("   First difference at byte {}: expected {}, got {}", i, a, b);
                        break;
                    }
                }
            }
        }
        Err(e) => println!("   Reassembly failed: {:?}", e),
    }

    // Demonstrate duplicate handling
    println!("\n6. Duplicate Handling:");
    let mut reassembler2: Reassembler<4, 4096> = Reassembler::new(max_fragment_size, 5000);

    // Receive frame 0
    let (frame0, _) = Frame::deserialize(&serialized_frames[0]).unwrap();
    reassembler2.process_frame(&frame0, 0).unwrap();
    println!("   Received fragment 0 (first time)");

    // Receive frame 0 again (duplicate)
    let result = reassembler2.process_frame(&frame0, 0).unwrap();
    println!("   Received fragment 0 (duplicate) -> completed: {:?}", result);

    // Demonstrate timeout
    println!("\n7. Timeout Handling:");
    let mut reassembler3: Reassembler<4, 4096> = Reassembler::new(max_fragment_size, 100); // 100ms timeout

    // Receive only first fragment
    let (frame0, _) = Frame::deserialize(&serialized_frames[0]).unwrap();
    reassembler3.process_frame(&frame0, 0).unwrap();
    println!("   Received fragment 0 at time 0");
    println!("   Active entries: {}", reassembler3.active_count());

    // Cleanup after timeout
    let cleaned = reassembler3.cleanup_timeout(200); // 200ms later
    println!("   Cleanup at time 200ms -> removed {} entries", cleaned);
    println!("   Active entries: {}", reassembler3.active_count());

    println!("\n=== Fragmentation Example Complete ===");
}
