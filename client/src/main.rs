mod trans_client;
use env_logger;
// use std::path::PathBuf;
use trans_client::{TransClient, ClientTarget};

const DATA_SIZE: usize =  10 * 1024 * 1024; // 10 MB
const DEFAULT_SERVER_CID: u32 = 103;       // 默认2， qemu用103， pvm用3
const DEFAULT_SERVER_PORT: u32 = 1234;


#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // let target = ClientTarget::Unix(PathBuf::from("/tmp/yamux.sock"));
    let target = ClientTarget::Vsock { cid: DEFAULT_SERVER_CID, port: DEFAULT_SERVER_PORT };
    // let target = ClientTarget::Tcp("127.0.0.1:1234".parse().unwrap());

    let client = TransClient::new(target);

    // 发送指定大小的数据
    log::info!("Sending {} KB of data...", DATA_SIZE / 1024);
    let data: Vec<u8> = vec![0xAB; DATA_SIZE];
    client.send_message(&data).await;

}
