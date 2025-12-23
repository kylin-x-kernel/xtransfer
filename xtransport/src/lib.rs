#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod config;
pub mod error;
pub mod io;
pub mod protocol;
pub mod transport;

pub use error::{Error, Result};
pub use io::{Read, Write};
pub use config::{TransportConfig, MAGIC, VERSION, HEADER_SIZE, MESSAGE_HEAD_SIZE};
pub use transport::XTransport;


