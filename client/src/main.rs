mod yamux_client;
use std::path::PathBuf;
use yamux_client::{ClientTarget, YamuxClient};

#[tokio::main]
async fn main() {
    let target = ClientTarget::Unix(PathBuf::from("/tmp/yamux.sock"));

    let client = YamuxClient::new(target);
    // 生成 1MB 的数据
    let large_message = "a".repeat(1024 * 1024);
    println!("Sending {} bytes...", large_message.len());
    client.send_message(&large_message).await;
}
