// mod aigen;
mod yamux_server;
use yamux_server::{ServerTarget, YamuxServer};

use tokio_vsock::VMADDR_CID_ANY;

const DEFAULT_SERVER_PORT: u32 = 1234;

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    //use std::path::PathBuf;
    //let target = ServerTarget::Unix(PathBuf::from("/tmp/yamux.sock"));
    
    // CID 3 usually refers to the host in some virtualization environments, or use VMADDR_CID_ANY (-1U)
    let target = ServerTarget::Vsock(VMADDR_CID_ANY, DEFAULT_SERVER_PORT);

    let server = YamuxServer::new(target);
    server.run().await;
}
