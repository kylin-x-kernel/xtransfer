use futures::future::poll_fn;
use futures::io::{AsyncReadExt, AsyncWriteExt};
use std::path::PathBuf;
use tokio::net::UnixListener;
use tokio_util::compat::TokioAsyncReadCompatExt;
use yamux::{Config, Connection, Mode};

pub enum ServerTarget {
    Unix(PathBuf),
}

pub struct YamuxServer {
    target: ServerTarget,
}

impl YamuxServer {
    pub fn new(target: ServerTarget) -> Self {
        Self { target }
    }

    pub async fn run(&self) {
        match &self.target {
            ServerTarget::Unix(path) => {
                if path.exists() {
                    std::fs::remove_file(path).ok();
                }
                let listener = UnixListener::bind(path).expect("Failed to bind Unix Socket");
                println!("Server listening on Unix Socket {:?}", path);
                loop {
                    let (stream, _) = listener.accept().await.expect("Failed to accept");
                    self.handle_connection(stream.compat());
                }
            }
        }
    }

    fn handle_connection<T>(&self, stream: T)
    where
        T: futures::io::AsyncRead + futures::io::AsyncWrite + Unpin + Send + 'static,
    {
        tokio::spawn(async move {
            let config = Config::default();
            let mut conn = Connection::new(stream, config, Mode::Server);
            loop {
                match poll_fn(|cx| conn.poll_next_inbound(cx)).await {
                    Some(Ok(mut stream)) => {
                        tokio::spawn(async move {
                            let mut buf = Vec::new();
                            // 读取直到流关闭 (EOF)
                            stream.read_to_end(&mut buf).await.expect("Failed to read");
                            println!("[Server] Received {} bytes", buf.len());

                            let reply = format!("Ack: received {} bytes", buf.len());
                            stream
                                .write_all(reply.as_bytes())
                                .await
                                .expect("Failed to write");
                            stream.close().await.expect("Failed to close");
                        });
                    }
                    Some(Err(e)) => {
                        eprintln!("Connection error: {}", e);
                        break;
                    }
                    None => break,
                }
            }
        });
    }
}
