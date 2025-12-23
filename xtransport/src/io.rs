use crate::{Error, Result};

pub trait Read {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;
    
    fn read_exact(&mut self, mut buf: &mut [u8]) -> Result<()> {
        while !buf.is_empty() {
            match self.read(buf) {
                Ok(0) => break,
                Ok(n) => {
                    let tmp = buf;
                    buf = &mut tmp[n..];
                }
                Err(e) => return Err(e),
            }
        }
        if !buf.is_empty() {
            Err(Error::new(crate::error::ErrorKind::UnexpectedEof))
        } else {
            Ok(())
        }
    }
}

pub trait Write {
    fn write(&mut self, buf: &[u8]) -> Result<usize>;
    fn flush(&mut self) -> Result<()>;
    
    fn write_all(&mut self, mut buf: &[u8]) -> Result<()> {
        while !buf.is_empty() {
            match self.write(buf) {
                Ok(0) => {
                    return Err(Error::new(crate::error::ErrorKind::WriteZero));
                }
                Ok(n) => buf = &buf[n..],
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

// Blanket implementations for std types that implement std::io::{Read, Write}
#[cfg(feature = "std")]
impl<T: std::io::Read> Read for T {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        std::io::Read::read(self, buf)
            .map_err(|e| Error::new(match e.kind() {
                std::io::ErrorKind::UnexpectedEof => crate::error::ErrorKind::UnexpectedEof,
                std::io::ErrorKind::Interrupted => crate::error::ErrorKind::Interrupted,
                _ => crate::error::ErrorKind::Other,
            }))
    }
}

#[cfg(feature = "std")]
impl<T: std::io::Write> Write for T {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        std::io::Write::write(self, buf)
            .map_err(|e| Error::new(match e.kind() {
                std::io::ErrorKind::WriteZero => crate::error::ErrorKind::WriteZero,
                std::io::ErrorKind::Interrupted => crate::error::ErrorKind::Interrupted,
                _ => crate::error::ErrorKind::Other,
            }))
    }
    
    fn flush(&mut self) -> Result<()> {
        std::io::Write::flush(self)
            .map_err(|_| Error::new(crate::error::ErrorKind::Other))
    }
}
