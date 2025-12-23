use std::os::unix::net::UnixStream;
use std::time::Instant;
use xtransport::XTransport;

const DATA_SIZE: usize = 1000 * 1024 * 1024; // 100 MB
const SOCKET_PATH: &str = "/tmp/xtransfer.sock";

fn main() {
    env_logger::init();

    println!("Connecting to server at {}...", SOCKET_PATH);
    let stream = UnixStream::connect(SOCKET_PATH).expect("Failed to connect to server");
    println!("Connected!");

    let mut transport = XTransport::new(stream);

    // Send 100MB data
    println!("Sending {} MB of data...", DATA_SIZE / 1024 / 1024);
    let data = vec![0xAB; DATA_SIZE];
    
    let start = Instant::now();
    transport.send_message(&data).expect("Failed to send message");
    let elapsed = start.elapsed();
    let speed = (DATA_SIZE as f64 / 1024.0 / 1024.0) / elapsed.as_secs_f64();
    
    println!("=== Send Complete ===");
    println!("Total sent: {} MB", DATA_SIZE / 1024 / 1024);
    println!("Time: {:.2} seconds", elapsed.as_secs_f64());
    println!("Speed: {:.2} MB/s", speed);

    // Receive data from server
    println!("\nReceiving data from server...");
    let start = Instant::now();
    let recv_data = transport.recv_message().expect("Failed to receive message");
    let elapsed = start.elapsed();
    let speed = (recv_data.len() as f64 / 1024.0 / 1024.0) / elapsed.as_secs_f64();
    
    println!("\n=== Receive Complete ===");
    println!("Total received: {} MB", recv_data.len() / 1024 / 1024);
    println!("Time: {:.2} seconds", elapsed.as_secs_f64());
    println!("Speed: {:.2} MB/s", speed);
}
