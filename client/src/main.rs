mod yamux_client;
use yamux_client::{ClientTarget, YamuxClient};

pub const DEFAULT_SERVER_CID: u32 = 3;  // 默认连接 Host (CID=3)
pub const DEFAULT_SERVER_PORT: u32 = 1234;

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    //use std::path::PathBuf;
    //let target = ClientTarget::Unix(PathBuf::from("/tmp/yamux.sock"));

    let target = ClientTarget::Vsock(DEFAULT_SERVER_CID, DEFAULT_SERVER_PORT);
    let client = YamuxClient::new(target);
    // Generate 1MB of data
    let large_message = "a".repeat(1024 * 1);
    log::info!("Sending {} bytes...", large_message.len());
    client.send_message(&large_message).await;
}
