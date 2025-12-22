**Yamux 极简使用教程**

这份教程将剥离复杂的底层原理，专注于**“怎么用”**，让你快速上手。

---

### 核心概念速记

*   **Connection (连接)**: 物理连接（如 TCP），是高速公路。
*   **Stream (流)**: 逻辑连接（Yamux 创建的），是高速公路上的车道。
*   **Session (会话)**: 管理 Connection 和 Stream 的管家。

---

### 一、 服务端 (Server) 怎么写？

服务端的任务是：**监听物理连接 -> 升级为 Yamux 连接 -> 等待客户端发起流 -> 处理数据**。

#### 1. 准备物理连接
首先，你需要一个普通的 TCP 监听器。
```rust
let listener = TcpListener::bind("127.0.0.1:9000").await?;
let (socket, _) = listener.accept().await?;
```

#### 2. 升级为 Yamux 连接
把普通的 TCP `socket` 包装进 Yamux 的 `Connection` 中。
*   `Mode::Server`: 声明我是服务端。
*   `socket.compat()`: 必须做这一步适配（因为 Tokio 和 Yamux 的 IO 标准不同）。
```rust
use yamux::{Config, Connection, Mode};
use tokio_util::compat::TokioAsyncReadCompatExt; // 关键适配器

let mut conn = Connection::new(socket.compat(), Config::default(), Mode::Server);
```

#### 3. 循环接受逻辑流 (Stream)
Yamux 服务端是被动的，需要不断轮询（Poll）来接受客户端发起的新流。
```rust
use futures::future::poll_fn;

// 这是一个死循环，持续处理连接上的所有事件
loop {
    // poll_next_inbound 负责两件事：
    // 1. 维持连接心跳、流控（后台驱动）。
    // 2. 当客户端发起新流时，返回 Some(Ok(stream))。
    match poll_fn(|cx| conn.poll_next_inbound(cx)).await {
        Some(Ok(mut stream)) => {
            // 收到一个新流！开启一个新任务去处理它，不要阻塞主循环，否则卡住poll_next_inbound的执行
            tokio::spawn(async move {
                let mut buf = Vec::new();
                stream.read_to_end(&mut buf).await.unwrap();
                println!("收到数据: {:?}", buf);
            });
        }
        Some(Err(e)) => break, // 连接出错
        None => break,         // 连接关闭
    }
}
```

---

### 二、 客户端 (Client) 怎么写？

客户端的任务是：**建立物理连接 -> 升级为 Yamux 连接 -> 开启后台驱动 -> 主动发起流 -> 发送数据**。

#### 1. 建立物理连接
```rust
let socket = TcpStream::connect("127.0.0.1:9000").await?;
```

#### 2. 升级为 Yamux 连接
和服务端一样，只是模式改为 `Mode::Client`。
```rust
let mut conn = Connection::new(socket.compat(), Config::default(), Mode::Client);
```

#### 3. 开启新流 (Open Stream)
告诉 Yamux：“我要发数据，给我开个频道”。
```rust
let mut stream = poll_fn(|cx| conn.poll_new_outbound(cx)).await?;
```

#### 4. 关键：启动后台驱动 (Driver)
这是新手最容易漏掉的一步！客户端拿到 `stream` 后，必须把 `conn`（连接管理器）扔到后台运行，否则数据发不出去。
```rust
tokio::spawn(async move {
    loop {
        // 客户端通常不期待服务端主动发起流，所以这里只负责驱动连接
        match poll_fn(|cx| conn.poll_next_inbound(cx)).await {
            Some(Ok(_)) => {}, // 忽略服务端发起的流（视业务而定）
            _ => break,
        }
    }
});
```

#### 5. 像操作文件一样操作流
现在你可以拿着第3步得到的 `stream` 随意读写了。
```rust
use futures::io::AsyncWriteExt;

// 发送数据
stream.write_all(b"Hello Yamux").await?;

// 告诉服务端：我写完了（发送 EOF）
stream.close().await?; 
```

---

### 三、 总结：最简模板

| 步骤 | 服务端 (Server) | 客户端 (Client) |
| :--- | :--- | :--- |
| **1. 底层 IO** | `TcpListener::accept()` | `TcpStream::connect()` |
| **2. 包装** | `Connection::new(..., Mode::Server)` | `Connection::new(..., Mode::Client)` |
| **3. 驱动方式** | 在主循环中 `await` `poll_next_inbound` | `tokio::spawn` 一个任务去跑 `poll_next_inbound` |
| **4. 业务逻辑** | 被动接收：`Some(Ok(stream))` | 主动发起：`conn.poll_new_outbound()` |
| **5. 数据读写** | `stream.read_to_end()` | `stream.write_all()` |

只要记住这个流程，你就可以在任何支持 `AsyncRead + AsyncWrite` 的底层协议（TCP, Unix Socket, WebSocket, QUIC 等）上使用 Yamux 了。