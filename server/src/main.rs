use std::os::unix::net::UnixListener;
use std::time::Instant;
use xtransport::XTransport;

const DATA_SIZE: usize = 100 * 1024 * 1024; // 100 MB
const SOCKET_PATH: &str = "/tmp/xtransfer.sock";

fn main() {
    env_logger::init();

    // Remove socket file if it exists
    let _ = std::fs::remove_file(SOCKET_PATH);

    println!("Starting server on {}...", SOCKET_PATH);
    let listener = UnixListener::bind(SOCKET_PATH).expect("Failed to bind to socket");
    println!("Server listening on {}", SOCKET_PATH);

    // Accept client connection
    let (stream, _) = listener.accept().expect("Failed to accept connection");
    println!("Client connected");
    
    let mut transport = XTransport::new(stream);

    // Receive data from client
    println!("Receiving data from client...");
    let start = Instant::now();
    let recv_data = transport.recv_message().expect("Failed to receive message");
    let elapsed = start.elapsed();
    let speed = (recv_data.len() as f64 / 1024.0 / 1024.0) / elapsed.as_secs_f64();
    
    println!("\n=== Receive Complete ===");
    println!("Total received: {} MB", recv_data.len() / 1024 / 1024);
    println!("Time: {:.2} seconds", elapsed.as_secs_f64());
    println!("Speed: {:.2} MB/s", speed);

    // Send 100MB data back
    println!("\nSending {} MB of data back...", DATA_SIZE / 1024 / 1024);
    let data = vec![0xCD; DATA_SIZE];
    
    let start = Instant::now();
    transport.send_message(&data).expect("Failed to send message");
    let elapsed = start.elapsed();
    let speed = (DATA_SIZE as f64 / 1024.0 / 1024.0) / elapsed.as_secs_f64();
    
    println!("\n=== Send Complete ===");
    println!("Total sent: {} MB", DATA_SIZE / 1024 / 1024);
    println!("Time: {:.2} seconds", elapsed.as_secs_f64());
    println!("Speed: {:.2} MB/s", speed);
    
    println!("Client handler finished");
}
