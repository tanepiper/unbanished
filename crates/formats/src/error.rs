use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Input ended before a read completed.
    Truncated { at: usize, wanted: usize },
    /// Structural check failed (bad marker, size mismatch, unread bytes...).
    Format(String),
    /// Data is valid but uses a variant this crate does not handle yet.
    Unsupported(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Truncated { at, wanted } => {
                write!(f, "truncated: wanted {wanted} bytes at {at:#x}")
            }
            Error::Format(m) => write!(f, "format error: {m}"),
            Error::Unsupported(m) => write!(f, "unsupported: {m}"),
        }
    }
}

impl std::error::Error for Error {}

pub(crate) fn format_err<T>(msg: impl Into<String>) -> Result<T> {
    Err(Error::Format(msg.into()))
}
