//! # XTransport - A Reliable Transport Protocol
//!
//! XTransport is a `no_std` compatible reliable transport protocol implementation
//! that provides:
//!
//! - **Packet fragmentation**: Large messages are split into smaller frames
//! - **Reliable delivery**: Automatic retransmission of lost packets
//! - **Out-of-order reassembly**: Packets arriving out of order are correctly reordered
//! - **CRC32 checksum**: Data integrity verification
//! - **Custom transport support**: Works with any transport implementing Read/Write traits
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │                    Application Layer                     │
//! ├─────────────────────────────────────────────────────────┤
//! │                    Protocol Layer                        │
//! │  ┌─────────────┐ ┌─────────────┐ ┌─────────────────┐   │
//! │  │ Reassembler │ │ Retransmit  │ │ Sliding Window  │   │
//! │  └─────────────┘ └─────────────┘ └─────────────────┘   │
//! ├─────────────────────────────────────────────────────────┤
//! │                    Frame Layer                           │
//! │  ┌─────────────┐ ┌─────────────┐ ┌─────────────────┐   │
//! │  │  Framing    │ │  Checksum   │ │   Sequencing    │   │
//! │  └─────────────┘ └─────────────┘ └─────────────────┘   │
//! ├─────────────────────────────────────────────────────────┤
//! │                    Transport Layer                       │
//! │  ┌─────────────────────────────────────────────────┐   │
//! │  │            Custom Transport (Read/Write)         │   │
//! │  └─────────────────────────────────────────────────┘   │
//! └─────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Example
//!
//! ```rust,ignore
//! use xtransport::{Protocol, Config};
//!
//! let config = Config::default();
//! let mut protocol = Protocol::new(transport, config);
//!
//! // Send data
//! protocol.send(b"Hello, World!")?;
//!
//! // Receive data
//! let mut buf = [0u8; 1024];
//! let n = protocol.recv(&mut buf)?;
//! ```

#![no_std]
#![deny(unsafe_code)]
#![warn(missing_docs)]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod core;
pub mod buffer;
pub mod reliable;
pub mod transport;
pub mod channel;
pub mod config;
pub mod error;
pub mod protocol;

// Re-export commonly used types
pub use core::{Crc32, Frame, FrameType, Packet, FRAME_HEADER_SIZE};
pub use config::Config;
pub use error::{Error, Result};
pub use protocol::{Protocol, ProtocolBuilder};
pub use transport::Transport;

/// Protocol version
pub const VERSION: u8 = 1;

/// Maximum frame size (including header, must be > FRAME_HEADER_SIZE)
pub const MAX_FRAME_SIZE: usize = 1048;

/// Maximum packet size
pub const MAX_PACKET_SIZE: usize = 65535;

/// Default window size
pub const DEFAULT_WINDOW_SIZE: u16 = 64;

/// Default retransmit timeout in milliseconds
pub const DEFAULT_RETRANSMIT_TIMEOUT_MS: u32 = 1000;

/// Default maximum retransmit attempts
pub const DEFAULT_MAX_RETRANSMIT: u8 = 5;
