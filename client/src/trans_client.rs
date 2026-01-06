use futures::future::poll_fn;
use futures::io::{AsyncRead, AsyncWrite};
use futures::{AsyncReadExt, AsyncWriteExt};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Instant;
use tokio::net::{TcpStream, UnixStream};
use tokio_util::compat::TokioAsyncReadCompatExt;
use tokio_vsock::{VsockAddr, VsockStream};
use yamux::{Config, Connection, Mode};
use log::*;

#[allow(dead_code)]
#[derive(Debug)]
pub enum ClientTarget {
    Unix(PathBuf),
    Tcp(SocketAddr),
    Vsock { cid: u32, port: u32 },
}

pub struct TransClient {
    target: ClientTarget,
}

impl TransClient {
    pub fn new(target: ClientTarget) -> Self {
        Self { target }
    }

    pub async fn send_message(&self, message: &[u8]) {
        info!("Connecting to target: {:?}", self.target);
        match &self.target {
            ClientTarget::Unix(path) => {
                let stream = UnixStream::connect(path)
                    .await
                    .expect("Failed to connect Unix Socket");
                info!("Unix socket connected.");
                self.process_stream(stream.compat(), message).await;
            }
            ClientTarget::Tcp(addr) => {
                let stream = TcpStream::connect(addr)
                    .await
                    .expect("Failed to connect TCP Socket");
                info!("TCP socket connected.");
                self.process_stream(stream.compat(), message).await;
            }
            ClientTarget::Vsock { cid, port } => {
                let stream = VsockStream::connect(VsockAddr::new(*cid, *port))
                    .await
                    .expect("Failed to connect Vsock Socket");
                info!("Vsock socket connected.");
                self.process_stream(stream.compat(), message).await;
            }
        }
    }

    async fn process_stream<T>(&self, stream: T, message: &[u8])
    where
        T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        // Initialize Yamux connection
        let config = Config::default();
        let mut conn = Connection::new(stream, config, Mode::Client);

        // Open a logical stream
        info!("Opening Yamux stream...");
        let mut yamux_stream = poll_fn(|cx| conn.poll_new_outbound(cx))
            .await
            .expect("Failed to open stream");
        info!("Yamux stream opened.");

        // Spawn the connection driver
        tokio::spawn(async move {
            loop {
                // poll_next_inbound will continuously read underlying data and parse Yamux frames
                // Connection 提供的 API 是低级的 Poll 接口（主要为了兼容 Stream trait 或者底层的 Future 机制）。
                // poll_fn 把它包装成一个可以在 async 块里 .await 的 Future。
                match poll_fn(|cx: &mut std::task::Context<'_>| conn.poll_next_inbound(cx)).await {
                    Some(Ok(_)) => {
                        // We don't expect inbound streams in this example, but we must drive the connection
                    }
                    Some(Err(e)) => {
                        error!("Connection error: {}", e);
                        break;
                    }
                    None => break,
                }
            }
            info!("Connection closed");
        });

        // Send message and test performance
        let start = Instant::now();
        yamux_stream
            .write_all(message)
            .await
            .expect("Failed to send message");
        yamux_stream.close().await.expect("Failed to close stream"); // Close the write end to notify the Server that data sending is complete
        let elapsed = start.elapsed();
        let speed = (message.len() as f64 / 1024.0) / elapsed.as_secs_f64();
        info!("=== Send Complete ===");
        info!("Total sent: {} KB", message.len() / 1024);
        info!("Time: {:.2} seconds", elapsed.as_secs_f64());
        info!("Speed: {:.2} KB/s", speed);
        info!("");
        info!("");

        // Read reply
        let start = Instant::now();
        let mut buf = Vec::new();
        yamux_stream
            .read_to_end(&mut buf)
            .await
            .expect("Failed to read reply");
        let elapsed = start.elapsed();
        let speed = (buf.len() as f64 / 1024.0) / elapsed.as_secs_f64();
        info!("=== Receive Complete ===");
        info!("Total received: {} KB", buf.len() / 1024);
        info!("Time: {:.2} seconds", elapsed.as_secs_f64());
        info!("Speed: {:.2} KB/s", speed);


    }
}