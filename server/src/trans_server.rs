
use futures::future::poll_fn;
use futures::io::{AsyncRead, AsyncWrite};
use futures::{AsyncReadExt, AsyncWriteExt};
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::net::{TcpListener, UnixListener};
use tokio_util::compat::TokioAsyncReadCompatExt;
use tokio_vsock::{VsockAddr, VsockListener};
use yamux::{Config, Connection, Mode};

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
                log::info!("Server listening on Unix Socket {:?}", path);
                loop {
                    let (stream, _) = listener.accept().await.expect("Failed to accept");
                    log::info!("Accepted Unix connection");
                    tokio::spawn(Self::handle_connection(stream.compat()));
                    log::info!("Spawned task to handle Unix connection");
                }
            }
            ServerTarget::Tcp(addr) => {
                let listener = TcpListener::bind(addr).await.expect("Failed to bind TCP Socket");
                log::info!("Server listening on TCP {:?}", addr);
                loop {
                    let (stream, peer) = listener.accept().await.expect("Failed to accept");
                    log::info!("Accepted TCP connection from {:?}", peer);
                    tokio::spawn(Self::handle_connection(stream.compat()));
                }
            }
            ServerTarget::Vsock { cid, port } => {
                let listener = VsockListener::bind(VsockAddr::new(*cid, *port)).expect("Failed to bind Vsock Socket");
                log::info!("Server listening on Vsock CID:{} Port:{}", cid, port);
                loop {
                    let (stream, addr) = listener.accept().await.expect("Failed to accept");
                    log::info!("Accepted Vsock connection from {:?}", addr);
                    tokio::spawn(Self::handle_connection(stream.compat()));
                    log::info!("Spawned task to handle Vsock connection");
                }
            }
        }
    }

    /// Handles a new incoming connection using the Yamux protocol.
    /// This function drives the Yamux connection and handles logical streams spawned from it.
    pub async fn handle_connection<T>(stream: T)
    where
        T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        log::info!("Starting Yamux handshake...");
        let config = Config::default();
        let mut conn = Connection::new(stream, config, Mode::Server);

        loop {
            // Poll for new inbound logical streams created by the client
            match poll_fn(|cx| conn.poll_next_inbound(cx)).await {
                Some(Ok(stream)) => {
                    // Spawn a new task to handle this specific logical stream independently
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_stream(stream).await {
                            log::error!("[Yamux] Stream error: {}", e);
                        }
                    });
                }
                Some(Err(e)) => {
                    log::error!("Connection error: {}", e);
                    break;
                }
                None => {
                    log::info!("Connection closed by remote");
                    break;
                }
            }
            log::info!("Yamux connection polling for new inbound streams...");
        }

        log::info!("Yamux connection handler exiting");
    }

    /// Handles a single logical Yamux stream.
    /// Reads a message and sends an acknowledgment back.
    async fn handle_stream<S>(mut stream: S) -> std::io::Result<()>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        log::info!("[Yamux] New stream opened");
        let mut buf = Vec::new();

        // Read all data until EOF
        match stream.read_to_end(&mut buf).await {
            Ok(n) => {
                if n == 0 {
                    log::info!("[Yamux] Stream closed empty");
                    return Ok(());
                }

                let msg = String::from_utf8_lossy(&buf);
                log::info!("[Yamux] Received: {}", msg);

                // Send reply
                let reply = format!("Ack: {}", msg);
                stream.write_all(reply.as_bytes()).await?;
                stream.close().await?;

                log::info!("[Yamux] Reply sent and stream closed");
            }
            Err(e) => {
                return Err(e);
            }
        }
        Ok(())
    }


    


}

