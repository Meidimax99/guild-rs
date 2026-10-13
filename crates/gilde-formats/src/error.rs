use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("unexpected end of data at offset {offset} (wanted {wanted} bytes)")]
    Eof { offset: usize, wanted: usize },
    #[error("bad magic: expected {expected:?}, found {found:?}")]
    BadMagic { expected: &'static [u8], found: Vec<u8> },
    #[error("unknown tag 0x{tag:02x} at offset {offset} in {context}")]
    UnknownTag { tag: u8, offset: usize, context: &'static str },
    #[error("file not found: {0}")]
    NotFound(PathBuf),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;
