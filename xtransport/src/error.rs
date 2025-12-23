use core::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    InvalidMagic,
    InvalidVersion,
    CrcMismatch,
    UnexpectedEof,
    InvalidPacket,
    WriteZero,
    Interrupted,
    Other,
}

#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
}

impl Error {
    pub fn new(kind: ErrorKind) -> Self {
        Error { kind }
    }

    pub fn kind(&self) -> ErrorKind {
        self.kind
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            ErrorKind::InvalidMagic => write!(f, "Invalid magic number"),
            ErrorKind::InvalidVersion => write!(f, "Invalid protocol version"),
            ErrorKind::CrcMismatch => write!(f, "CRC checksum mismatch"),
            ErrorKind::UnexpectedEof => write!(f, "Unexpected end of file"),
            ErrorKind::InvalidPacket => write!(f, "Invalid packet"),
            ErrorKind::WriteZero => write!(f, "Write zero bytes"),
            ErrorKind::Interrupted => write!(f, "Operation interrupted"),
            ErrorKind::Other => write!(f, "Other error"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {}

#[cfg(feature = "std")]
impl From<Error> for std::io::Error {
    fn from(err: Error) -> std::io::Error {
        let kind = match err.kind {
            ErrorKind::UnexpectedEof => std::io::ErrorKind::UnexpectedEof,
            ErrorKind::WriteZero => std::io::ErrorKind::WriteZero,
            ErrorKind::Interrupted => std::io::ErrorKind::Interrupted,
            _ => std::io::ErrorKind::Other,
        };
        std::io::Error::new(kind, err)
    }
}

pub type Result<T> = core::result::Result<T, Error>;
