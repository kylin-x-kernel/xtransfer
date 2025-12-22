//! Transport layer abstraction.
//!
//! This module provides the `Transport` trait that allows the protocol
//! to work with any underlying transport mechanism (TCP, UDP, serial, etc.)
//!
//! # Implementations
//!
//! - `LoopbackTransport`: In-memory loopback for testing
//! - `NullTransport`: Discards all data (testing)
//! - `BufferedTransport`: Adds buffering to any transport
//! - `StdTransport`: Wraps std::io Read/Write types (requires `std` feature)
//!
//! # Example
//!
//! ```rust,ignore
//! use xtransport::transport::{Transport, LoopbackTransport};
//!
//! let mut transport = LoopbackTransport::<1024>::new();
//! transport.write(b"Hello")?;
//!
//! let mut buf = [0u8; 32];
//! let n = transport.read(&mut buf)?;
//! assert_eq!(&buf[..n], b"Hello");
//! ```

use crate::error::{Error, Result};
use crate::buffer::RingBuffer;

/// Transport trait for reading and writing raw bytes.
///
/// This is the main abstraction for underlying transport mechanisms.
/// Implement this trait to use custom transports with the protocol.
pub trait Transport {
    /// Reads bytes into the buffer.
    ///
    /// Returns the number of bytes read, or an error.
    /// Returns `Error::WouldBlock` if no data is available.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;

    /// Writes bytes from the buffer.
    ///
    /// Returns the number of bytes written, or an error.
    fn write(&mut self, buf: &[u8]) -> Result<usize>;

    /// Flushes any buffered data.
    fn flush(&mut self) -> Result<()>;

    /// Writes all bytes, retrying until complete.
    fn write_all(&mut self, buf: &[u8]) -> Result<()> {
        let mut written = 0;
        while written < buf.len() {
            let n = self.write(&buf[written..])?;
            if n == 0 {
                return Err(Error::IoError);
            }
            written += n;
        }
        self.flush()
    }
}

/// A loopback transport for testing.
///
/// Data written is immediately available to be read back.
#[derive(Debug)]
pub struct LoopbackTransport<const N: usize> {
    buffer: RingBuffer<N>,
}

impl<const N: usize> LoopbackTransport<N> {
    /// Creates a new loopback transport with the given buffer size.
    pub fn new() -> Self {
        Self {
            buffer: RingBuffer::new(),
        }
    }

    /// Returns the number of bytes available to read.
    pub fn available(&self) -> usize {
        self.buffer.len()
    }

    /// Clears all buffered data.
    pub fn clear(&mut self) {
        while self.buffer.read(&mut [0u8; 256]) > 0 {}
    }
}

impl<const N: usize> Default for LoopbackTransport<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Transport for LoopbackTransport<N> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if self.buffer.is_empty() {
            return Err(Error::WouldBlock);
        }
        Ok(self.buffer.read(buf))
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        Ok(self.buffer.write(buf))
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

/// A null transport that discards all writes and returns nothing.
///
/// Useful for testing or measuring overhead.
#[derive(Debug, Default)]
pub struct NullTransport {
    bytes_written: usize,
}

impl NullTransport {
    /// Creates a new null transport.
    pub fn new() -> Self {
        Self { bytes_written: 0 }
    }

    /// Returns the total number of bytes written.
    pub fn bytes_written(&self) -> usize {
        self.bytes_written
    }

    /// Resets the byte counter.
    pub fn reset(&mut self) {
        self.bytes_written = 0;
    }
}

impl Transport for NullTransport {
    fn read(&mut self, _buf: &mut [u8]) -> Result<usize> {
        Err(Error::WouldBlock)
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.bytes_written += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

/// A buffered transport wrapper.
///
/// Adds read and write buffering to any underlying transport.
#[derive(Debug)]
pub struct BufferedTransport<T, const RS: usize, const WS: usize> {
    inner: T,
    read_buf: RingBuffer<RS>,
    write_buf: RingBuffer<WS>,
}

impl<T: Transport, const RS: usize, const WS: usize> BufferedTransport<T, RS, WS> {
    /// Creates a new buffered transport wrapping the given transport.
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            read_buf: RingBuffer::new(),
            write_buf: RingBuffer::new(),
        }
    }

    /// Returns a reference to the inner transport.
    pub fn inner(&self) -> &T {
        &self.inner
    }

    /// Returns a mutable reference to the inner transport.
    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    /// Consumes the buffered transport and returns the inner transport.
    pub fn into_inner(self) -> T {
        self.inner
    }

    /// Returns the number of bytes buffered for reading.
    pub fn read_buffered(&self) -> usize {
        self.read_buf.len()
    }

    /// Returns the number of bytes buffered for writing.
    pub fn write_buffered(&self) -> usize {
        self.write_buf.len()
    }

    /// Fills the read buffer from the inner transport.
    fn fill_read_buffer(&mut self) -> Result<()> {
        if self.read_buf.is_full() {
            return Ok(());
        }

        let mut temp = [0u8; 256];
        match self.inner.read(&mut temp) {
            Ok(n) => {
                self.read_buf.write(&temp[..n]);
                Ok(())
            }
            Err(Error::WouldBlock) => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Drains the write buffer to the inner transport.
    fn drain_write_buffer(&mut self) -> Result<()> {
        if self.write_buf.is_empty() {
            return Ok(());
        }

        let mut temp = [0u8; 256];
        while !self.write_buf.is_empty() {
            let n = self.write_buf.peek(&mut temp);
            let written = self.inner.write(&temp[..n])?;
            self.write_buf.skip(written);
        }
        Ok(())
    }
}

impl<T: Transport, const RS: usize, const WS: usize> Transport for BufferedTransport<T, RS, WS> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        // Try to fill read buffer first
        let _ = self.fill_read_buffer();

        if self.read_buf.is_empty() {
            return Err(Error::WouldBlock);
        }

        Ok(self.read_buf.read(buf))
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let written = self.write_buf.write(buf);
        Ok(written)
    }

    fn flush(&mut self) -> Result<()> {
        self.drain_write_buffer()?;
        self.inner.flush()
    }
}

/// Wrapper for std::io types.
#[cfg(feature = "std")]
#[derive(Debug)]
pub struct StdTransport<T> {
    inner: T,
}

#[cfg(feature = "std")]
impl<T> StdTransport<T> {
    /// Creates a new StdTransport wrapping the given type.
    pub fn new(inner: T) -> Self {
        Self { inner }
    }

    /// Returns a reference to the inner type.
    pub fn inner(&self) -> &T {
        &self.inner
    }

    /// Returns a mutable reference to the inner type.
    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.inner
    }

    /// Consumes the wrapper and returns the inner type.
    pub fn into_inner(self) -> T {
        self.inner
    }
}

#[cfg(feature = "std")]
impl<T: std::io::Read + std::io::Write> Transport for StdTransport<T> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        std::io::Read::read(&mut self.inner, buf).map_err(|_| Error::IoError)
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        std::io::Write::write(&mut self.inner, buf).map_err(|_| Error::IoError)
    }

    fn flush(&mut self) -> Result<()> {
        std::io::Write::flush(&mut self.inner).map_err(|_| Error::IoError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loopback() {
        let mut transport: LoopbackTransport<1024> = LoopbackTransport::new();

        let data = b"Hello, World!";
        let written = transport.write(data).unwrap();
        assert_eq!(written, data.len());

        let mut buf = [0u8; 32];
        let read = transport.read(&mut buf).unwrap();
        assert_eq!(read, data.len());
        assert_eq!(&buf[..read], data);
    }

    #[test]
    fn test_null_transport() {
        let mut transport = NullTransport::new();

        let data = b"Test data";
        let written = transport.write(data).unwrap();
        assert_eq!(written, data.len());
        assert_eq!(transport.bytes_written(), data.len());

        let mut buf = [0u8; 32];
        assert!(transport.read(&mut buf).is_err());
    }

    #[test]
    fn test_buffered_transport() {
        let inner: LoopbackTransport<1024> = LoopbackTransport::new();
        let mut transport: BufferedTransport<_, 256, 256> = BufferedTransport::new(inner);

        let data = b"Buffered test";
        transport.write(data).unwrap();
        transport.flush().unwrap();

        let mut buf = [0u8; 32];
        let n = transport.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], data);
    }
}
