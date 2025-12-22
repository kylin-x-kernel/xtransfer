// mod aigen;
mod yamux_server;
use std::path::PathBuf;
use yamux_server::{ServerTarget, YamuxServer};

#[tokio::main]
async fn main() {
    let target = ServerTarget::Unix(PathBuf::from("/tmp/yamux.sock"));

    let server = YamuxServer::new(target);
    server.run().await;
}
