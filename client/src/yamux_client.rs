use futures::future::poll_fn;
use futures::io::{AsyncReadExt, AsyncWriteExt};
use std::path::PathBuf;
use tokio::net::UnixStream;
use tokio_util::compat::TokioAsyncReadCompatExt;
use yamux::{Config, Connection, Mode};

pub enum ClientTarget {
    Unix(PathBuf),
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
        }
    }

    async fn process_stream<T>(&self, stream: T, message: &str)
    where
        T: futures::io::AsyncRead + futures::io::AsyncWrite + Unpin + Send + 'static,
    {
        // 初始化 Yamux 连接
        let config = Config::default();
        let mut conn = Connection::new(stream, config, Mode::Client);

        // 打开逻辑流 (Stream)
        let mut yamux_stream = poll_fn(|cx| conn.poll_new_outbound(cx))
            .await
            .expect("Failed to open stream");

        // Spawn the connection driver
        tokio::spawn(async move {
            loop {
                // poll_next_inbound 会不断读取底层 TCP 数据，解析 Yamux 帧
                match poll_fn(|cx: &mut std::task::Context<'_>| conn.poll_next_inbound(cx)).await {
                    Some(Ok(_)) => {
                        // We don't expect inbound streams in this example, but we must drive the connection
                    }
                    Some(Err(e)) => {
                        eprintln!("Connection error: {}", e);
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
        // 关闭写端，通知 Server 数据发送完毕
        yamux_stream.close().await.expect("Failed to close stream");

        let mut buf = Vec::new();
        yamux_stream
            .read_to_end(&mut buf)
            .await
            .expect("Failed to read reply");
        let reply = String::from_utf8_lossy(&buf);
        println!("[Client] Received: {}", reply);
    }
}
