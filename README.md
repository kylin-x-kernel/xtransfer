# XTransport Protocol

cargo build -p server --release --target aarch64-unknown-linux-musl


A reliable transport protocol implementation with `no_std` + `alloc` support.

## Protocol Design

### Packet Structure

**PacketHeader** (16 bytes):
- Magic: `0x58545250` ("XTRP")
- Version: `0x01`
- Type: Data(0) / MessageHead(1) / MessageData(2)
- Sequence: 4 bytes
- Length: 2 bytes (max 65520)
- CRC32: 4 bytes

**MessageHead** (32 bytes) - for large messages:
- Total Length: 8 bytes
- Message ID: 8 bytes
- Packet Count: 4 bytes
- Flags: 4 bytes
- Reserved: 8 bytes

### Message Types

- **Single Packet**: Data ≤ 64KB → one Data packet
- **Multi Packet**: Data > 64KB → MessageHead + multiple MessageData packets

### Features

- CRC32 validation
- Automatic fragmentation/reassembly
- Unix Domain Socket transport
- Custom Read/Write traits for no_std compatibility

## Usage

```rust
let stream = UnixStream::connect("/tmp/xtransfer.sock")?;
let mut transport = XTransport::new(stream);

// Send message
transport.send_message(&data)?;

// Receive message
let data = transport.recv_message()?;
```