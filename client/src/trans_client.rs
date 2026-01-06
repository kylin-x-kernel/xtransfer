use futures::future::poll_fn;
use futures::io::{AsyncRead, AsyncWrite};
use futures::{AsyncReadExt, AsyncWriteExt};
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::net::{TcpStream, UnixStream};
use tokio_util::compat::TokioAsyncReadCompatExt;
use tokio_vsock::{VsockAddr, VsockStream};
use yamux::{Config, Connection, Mode};

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

    pub async fn send_message(&self, message: &str) {
        log::info!("Connecting to target: {:?}", self.target);
        match &self.target {
            ClientTarget::Unix(path) => {
                let stream = UnixStream::connect(path)
                    .await
                    .expect("Failed to connect Unix Socket");
                log::info!("Unix socket connected.");
                self.process_stream(stream.compat(), message).await;
            }
            ClientTarget::Tcp(addr) => {
                let stream = TcpStream::connect(addr)
                    .await
                    .expect("Failed to connect TCP Socket");
                log::info!("TCP socket connected.");
                self.process_stream(stream.compat(), message).await;
            }
            ClientTarget::Vsock { cid, port } => {
                let stream = VsockStream::connect(VsockAddr::new(*cid, *port))
                    .await
                    .expect("Failed to connect Vsock Socket");
                log::info!("Vsock socket connected.");
                self.process_stream(stream.compat(), message).await;
            }
        }
    }

    async fn process_stream<T>(&self, stream: T, message: &str)
    where
        T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        // Initialize Yamux connection
        let config = Config::default();
        let mut conn = Connection::new(stream, config, Mode::Client);

        // Open a logical stream
        log::info!("Opening Yamux stream...");
        let mut yamux_stream = poll_fn(|cx| conn.poll_new_outbound(cx))
            .await
            .expect("Failed to open stream");
        log::info!("Yamux stream opened.");

        // Spawn the connection driver
        tokio::spawn(async move {
            loop {
                // poll_next_inbound will continuously read underlying data and parse Yamux frames
                match poll_fn(|cx: &mut std::task::Context<'_>| conn.poll_next_inbound(cx)).await {
                    Some(Ok(_)) => {
                        // We don't expect inbound streams in this example, but we must drive the connection
                    }
                    Some(Err(e)) => {
                        log::error!("Connection error: {}", e);
                        break;
                    }
                    None => break,
                }
            }
            log::info!("Connection closed");
        });

        // Send message
        log::info!("Sending message: {}", message);
        yamux_stream
            .write_all(message.as_bytes())
            .await
            .expect("Failed to send message");
        // Close the write end to notify the Server that data sending is complete
        yamux_stream.close().await.expect("Failed to close stream");

        // Read reply
        let mut buf = Vec::new();
        yamux_stream
            .read_to_end(&mut buf)
            .await
            .expect("Failed to read reply");
        let reply = String::from_utf8_lossy(&buf);
        log::info!("[Client] Received reply: {}", reply);
    }
}
