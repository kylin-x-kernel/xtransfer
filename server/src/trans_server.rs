use futures::future::poll_fn;
use futures::io::{AsyncRead, AsyncWrite};
use futures::{AsyncReadExt, AsyncWriteExt};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Instant;
use tokio::net::{TcpListener, UnixListener};
use tokio_util::compat::TokioAsyncReadCompatExt;
use tokio_vsock::{VsockAddr, VsockListener};
use yamux::{Config, Connection, Mode};
use log::*;

use crate::DATA_SIZE;

#[allow(dead_code)]
#[derive(Debug)]
pub enum ServerTarget {
    Unix(PathBuf),
    Tcp(SocketAddr),
    Vsock { cid: u32, port: u32 },
}

pub struct TransServer {
    target: ServerTarget,
}

impl TransServer {
    pub fn new(target: ServerTarget) -> Self {
        Self { target }
    }

    pub async fn run(&self) {
        match &self.target {
            ServerTarget::Unix(path) => {
                if path.exists() {
                    let _ = std::fs::remove_file(path);
                }
                let listener = UnixListener::bind(path).expect("Failed to bind Unix Socket");
                info!("Server listening on Unix Socket {:?}", path);
                loop {
                    let (stream, _) = listener.accept().await.expect("Failed to accept");
                    info!("Accepted Unix connection");
                    tokio::spawn(Self::handle_connection(stream.compat()));
                    info!("Spawned task to handle Unix connection");
                }
            }
            ServerTarget::Tcp(addr) => {
                let listener = TcpListener::bind(addr).await.expect("Failed to bind TCP Socket");
                info!("Server listening on TCP {:?}", addr);
                loop {
                    let (stream, peer) = listener.accept().await.expect("Failed to accept");
                    info!("Accepted TCP connection from {:?}", peer);
                    tokio::spawn(Self::handle_connection(stream.compat()));
                }
            }
            ServerTarget::Vsock { cid, port } => {
                let listener = VsockListener::bind(VsockAddr::new(*cid, *port)).expect("Failed to bind Vsock Socket");
                info!("Server listening on Vsock CID:{} Port:{}", cid, port);
                
                    let (stream, addr) = listener.accept().await.expect("Failed to accept");
                    info!("Accepted Vsock connection from {:?}", addr);
                    Self::handle_connection(stream.compat()).await;
                    info!("Spawned task to handle Vsock connection");
                
            }
        }
    }

    /// Handles a new incoming connection using the Yamux protocol.
    /// This function drives the Yamux connection and handles logical streams spawned from it.
    pub async fn handle_connection<T>(stream: T)
    where
        T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        info!("Starting Yamux handshake...");
        let config = Config::default();
        let mut conn = Connection::new(stream, config, Mode::Server);

        loop {
            // Poll for new inbound logical streams created by the client
            match poll_fn(|cx| conn.poll_next_inbound(cx)).await {
                Some(Ok(stream)) => {
                    // Spawn a new task to handle this specific logical stream independently
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_stream(stream).await {
                            error!("[Yamux] Stream error: {}", e);
                        }
                    });
                }
                Some(Err(e)) => {
                    error!("Connection error: {}", e);
                    break;
                }
                None => {
                    info!("Connection closed by remote");
                    break;
                }
            }
            info!("Yamux connection polling for new inbound streams...");
        }

        info!("Yamux connection handler exiting");
    }

    /// Handles a single logical Yamux stream.
    /// Reads a message and sends an acknowledgment back.
    async fn handle_stream<S>(mut stream: S) -> std::io::Result<()>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        info!("[Yamux] New stream opened");
        let mut buf = Vec::new();

        // Read all data until EOF
        let start = Instant::now();
        let n = stream.read_to_end(&mut buf).await.expect("Failed to read data");
        let elapsed = start.elapsed();
        let speed = (n as f64 / 1024.0) / elapsed.as_secs_f64();
        info!("=== Receive Complete ===");
        info!("Total received: {} KB", n / 1024);
        info!("Time: {:.2} seconds", elapsed.as_secs_f64());
        info!("Speed: {:.2} KB/s", speed);
        info!("");
        info!("");

        if n == 0 {
            info!("[Yamux] Stream closed empty");
            return Ok(());
        }

        // Send reply
        let start = Instant::now();
        let reply: Vec<u8> = vec![0xAB; DATA_SIZE];
        stream.write_all(&reply).await?;
        stream.flush().await?;
        stream.close().await?;
        let elapsed = start.elapsed();
        let speed = (DATA_SIZE as f64 / 1024.0) / elapsed.as_secs_f64();
        info!("=== Send Complete ===");
        info!("Total sent: {} KB", DATA_SIZE / 1024);
        info!("Time: {:.2} seconds", elapsed.as_secs_f64());
        info!("Speed: {:.2} KB/s", speed);
        info!("");
        info!("");

        info!("[Yamux] Reply sent and stream closed");

        Ok(())
        // // Force exit after sending reply (for testing purposes)
        // std::process::exit(0);
    }


    


}

