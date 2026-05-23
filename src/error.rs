use thiserror::Error;

#[derive(Debug, Error)]
pub enum BufrError {
    #[error("message is too short to be BUFR")]
    TooShort,
    #[error("missing BUFR marker")]
    MissingMagic,
    #[error("unsupported BUFR edition {0}; only edition 4 is implemented")]
    UnsupportedEdition(u8),
    #[error("declared length {declared} does not match buffer length {actual}")]
    LengthMismatch { declared: usize, actual: usize },
    #[error("section {section} at offset {offset} is truncated")]
    TruncatedSection { section: u8, offset: usize },
    #[error("section 5 terminator is not 7777")]
    BadTerminator,
    #[error("section 3 descriptor area has odd byte count")]
    BadDescriptorLength,
    #[error("descriptor {0:06} was not found in Table D")]
    MissingSequence(u32),
    #[error("delayed replication descriptor {0:06} is missing its replication factor or replicated descriptors")]
    DelayedReplication(u32),
    #[error("unsupported replication descriptor {0:06}")]
    UnsupportedReplication(u32),
    #[error("compressed BUFR data is not decoded yet")]
    CompressedDataUnsupported,
    #[error("descriptor {0:06} was not found in Table B")]
    MissingElement(u32),
    #[error("not enough bits while reading section 4")]
    BitstreamExhausted,
    #[error("csv/table parse error: {0}")]
    Table(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, BufrError>;
