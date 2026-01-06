mod trans_client;
use env_logger;
// use std::path::PathBuf;
use trans_client::{TransClient, ClientTarget};

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // let target = ClientTarget::Unix(PathBuf::from("/tmp/yamux.sock"));
    let target = ClientTarget::Vsock { cid: 103, port: 1234 };
    // let target = ClientTarget::Tcp("127.0.0.1:1234".parse().unwrap());

    let client = TransClient::new(target);
    client.send_message("Hello from Yamux Client!").await;
}