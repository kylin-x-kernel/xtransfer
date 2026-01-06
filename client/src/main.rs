use log::info;
use std::os::unix::net::UnixStream;
use vsock::{VsockAddr, VsockStream};
use std::time::Instant;
use xtransport::{TransportConfig, XTransport};

const DATA_SIZE: usize = 100 * 1024 * 1024; // 100 MB
const SOCKET_PATH: &str = "/tmp/xtransfer.sock";

const DEFAULT_SERVER_CID: u32 = 3;       // 默认2， qemu用103， pvm用3
const DEFAULT_SERVER_PORT: u32 = 1234;

fn main() {
    env_logger::init();

    // method 1  unix
    // info!("Connecting to server at {}...", SOCKET_PATH);
    // let stream = UnixStream::connect(SOCKET_PATH).expect("Failed to connect to server");
    // info!("Connected!");

    // method 2  vsock
    info!("Connecting to server at {}...", SOCKET_PATH);
    let addr = VsockAddr::new(DEFAULT_SERVER_CID, DEFAULT_SERVER_PORT);
    let stream = VsockStream::connect(&addr).expect("Failed to connect to server");
    info!("Connected!");

    let mut transport = XTransport::new(stream, TransportConfig::default().with_max_frame_size(1000));

    // Send 100MB data
    info!("Sending {} MB of data...", DATA_SIZE / 1024 / 1024);
    let data = vec![0xAB; DATA_SIZE];

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

    // Receive data from server
    info!("Receiving data from server...");
    let start = Instant::now();
    let recv_data = transport.recv_message().expect("Failed to receive message");
    let elapsed = start.elapsed();
    let speed = (recv_data.len() as f64 / 1024.0 / 1024.0) / elapsed.as_secs_f64();

    info!("=== Receive Complete ===");
    info!("Total received: {} MB", recv_data.len() / 1024 / 1024);
    info!("Time: {:.2} seconds", elapsed.as_secs_f64());
    info!("Speed: {:.2} MB/s", speed);
}
