use log::info;
use std::os::unix::net::UnixListener;
use std::time::Instant;
use xtransport::{TransportConfig, XTransport};

const DATA_SIZE: usize = 200 * 1000 * 1024; // 200 MB
const SOCKET_PATH: &str = "/tmp/xtransfer.sock";

fn main() {
    env_logger::init();

    // Remove socket file if it exists
    let _ = std::fs::remove_file(SOCKET_PATH);

    info!("Starting server on {}...", SOCKET_PATH);
    let listener = UnixListener::bind(SOCKET_PATH).expect("Failed to bind to socket");
    info!("Server listening on {}", SOCKET_PATH);

    // Accept client connection
    let (stream, _) = listener.accept().expect("Failed to accept connection");
    info!("Client connected");

    let mut transport = XTransport::new(
        stream,
        TransportConfig::default()
            .with_max_frame_size(2048)
            .with_ack(false),
    );

    // Receive data from client
    info!("Receiving data from client...");
    let start = Instant::now();
    let recv_data = transport.recv_message().expect("Failed to receive message");
    let elapsed = start.elapsed();
    let speed = (recv_data.len() as f64 / 1024.0 / 1024.0) / elapsed.as_secs_f64();

    info!("=== Receive Complete ===");
    info!("Total received: {} MB", recv_data.len() / 1024 / 1024);
    info!("Time: {:.2} seconds", elapsed.as_secs_f64());
    info!("Speed: {:.2} MB/s", speed);

    // Send 100MB data back
    info!("Sending {} MB of data back...", DATA_SIZE / 1024 / 1024);
    let data = vec![0xCD; DATA_SIZE];

    let start = Instant::now();
    transport
        .send_message(&data)
        .expect("Failed to send message");
    let elapsed = start.elapsed();
    let speed = (DATA_SIZE as f64 / 1024.0 / 1024.0) / elapsed.as_secs_f64();

    info!("=== Send Complete ===");
    info!("Total sent: {} MB", DATA_SIZE / 1024 / 1024);
    info!("Time: {:.2} seconds", elapsed.as_secs_f64());
    info!("Speed: {:.2} MB/s", speed);

    info!("Client handler finished");
}
