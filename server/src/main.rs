mod trans_server;

// use std::path::PathBuf;
use crate::trans_server::{ServerTarget, TransServer};
use tokio_vsock::VMADDR_CID_ANY;


#[tokio::main]
async fn main() -> std::io::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    // let target = ServerTarget::Unix(PathBuf::from("/tmp/yamux.sock"));
    let target = ServerTarget::Vsock { cid: VMADDR_CID_ANY, port: 1234 };
    // let target = ServerTarget::Tcp("127.0.0.1:1234".parse().unwrap());

    let server = TransServer::new(target);
    server.run().await;

    Ok(())  
}