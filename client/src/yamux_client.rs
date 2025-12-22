use futures::future::poll_fn;
use futures::io::{AsyncReadExt, AsyncWriteExt};
use std::path::PathBuf;
use tokio::net::UnixStream;
use tokio_vsock::{VsockStream, VsockAddr};
use tokio_util::compat::TokioAsyncReadCompatExt;
use yamux::{Config, Connection, Mode};

#[allow(dead_code)]
pub enum ClientTarget {
    Unix(PathBuf),
    Vsock(u32, u32),
}

pub struct YamuxClient {
    target: ClientTarget,
}

impl YamuxClient {
    pub fn new(target: ClientTarget) -> Self {
        Self { target }
    }

    pub async fn send_message(&self, message: &str) {
        match &self.target {
            ClientTarget::Unix(path) => {
                let stream = UnixStream::connect(path)
                    .await
                    .expect("Failed to connect Unix Socket");
                self.process_stream(stream.compat(), message).await;
            }
            ClientTarget::Vsock(cid, port) => {
                let addr = VsockAddr::new(*cid, *port);
                let stream = VsockStream::connect(addr)
                    .await
                    .expect("Failed to connect Vsock");
                log::info!("Connecting to Vsock CID: {} Port: {}", cid, port);
                self.process_stream(stream.compat(), message).await;
            }
        }
    }

    async fn process_stream<T>(&self, stream: T, message: &str)
    where
        T: futures::io::AsyncRead + futures::io::AsyncWrite + Unpin + Send + 'static,
    {
        log::info!("[Client] Connected to server, preparing to send message...");
        // Initialize Yamux connection
        let config = Config::default();

        let mut conn = Connection::new(stream, config, Mode::Client);

        // Open a logical stream
        let mut yamux_stream = poll_fn(|cx| conn.poll_new_outbound(cx))
            .await
            .expect("Failed to open stream");

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
        });

        yamux_stream
            .write_all(message.as_bytes())
            .await
            .expect("Failed to send message");
        // Close the write end to notify the Server that data sending is complete
        yamux_stream.close().await.expect("Failed to close stream");

        let mut buf = Vec::new();
        yamux_stream
            .read_to_end(&mut buf)
            .await
            .expect("Failed to read reply");
        let reply = String::from_utf8_lossy(&buf);
        log::info!("[Client] Received: {}", reply);
    }
}
