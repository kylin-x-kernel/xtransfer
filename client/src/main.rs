mod yamux_client;
use std::path::PathBuf;
use yamux_client::{ClientTarget, YamuxClient};

#[tokio::main]
async fn main() {
    env_logger::init();
    let target = ClientTarget::Unix(PathBuf::from("/tmp/yamux.sock"));

    let client = YamuxClient::new(target);
    // Generate 1MB of data
    let large_message = "a".repeat(1024 * 1024);
    log::info!("Sending {} bytes...", large_message.len());
    client.send_message(&large_message).await;
}
