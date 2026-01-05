use log::info;
// use std::os::unix::net::UnixListener;
use vsock::{VsockAddr, VsockListener, VMADDR_CID_ANY};
use std::time::Instant;
use xtransport::{TransportConfig, XTransport};

const DATA_SIZE: usize = 200 * 1024; // 200 KB
// const SOCKET_PATH: &str = "/tmp/xtransfer.sock";

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("trace")).init();

    // method 1 unix
    // Remove socket file if it exists
    // let _ = std::fs::remove_file(SOCKET_PATH);
    // info!("Starting server on {}...", SOCKET_PATH);
    // let listener = UnixListener::bind(SOCKET_PATH).expect("Failed to bind to socket");
    // info!("Server listening on {}", SOCKET_PATH);
    // // Accept client connection
    // let (stream, _) = listener.accept().expect("Failed to accept connection");
    // info!("Client connected");

    // method 2 vsock
    let addr = VsockAddr::new(VMADDR_CID_ANY, 1234);
    let listener = VsockListener::bind(&addr).expect("Failed to bind to vsock");
    info!("Server listening on {:?}", addr);
    let (stream, _) = listener.accept().expect("Failed to accept connection");
    info!("Client connected");

    let mut transport = XTransport::new(
        stream,
        TransportConfig::default()
            .with_max_frame_size(1024)
            .with_ack(false),
    );

    // Receive data from client
    info!("Receiving data from client...");
    let start = Instant::now();
    let recv_data = transport.recv_message().expect("Failed to receive message");
    
    let data = vec![0xAB; 10 * 1024 * 1024];
    // 判断是否全等
    if recv_data == data {
        info!("Data matches exactly");
    } else {
        info!("Data does not match");
    }
    
    let elapsed = start.elapsed();
    let speed = (recv_data.len() as f64 / 1024.0) / elapsed.as_secs_f64();

    info!("=== Receive Complete ===");
    info!("Total received: {} KB", recv_data.len() / 1024);
    info!("Time: {:.2} seconds", elapsed.as_secs_f64());
    info!("Speed: {:.2} KB/s", speed);

    // Send 100KB data back
    info!("Sending {} KB of data back...", DATA_SIZE / 1024);
    let data = vec![0xCD; DATA_SIZE];

    let start = Instant::now();
    transport
        .send_message(&data)
        .expect("Failed to send message");
    let elapsed = start.elapsed();
    let speed = (DATA_SIZE as f64 / 1024.0) / elapsed.as_secs_f64();

    info!("=== Send Complete ===");
    info!("Total sent: {} KB", DATA_SIZE / 1024);
    info!("Time: {:.2} seconds", elapsed.as_secs_f64());
    info!("Speed: {:.2} KB/s", speed);

    info!("Client handler finished");
}



