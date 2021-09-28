use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Unknown Error")]
    Unknown,
}

/// A result type bound to the standard orga error type.    
pub type Result<T> = std::result::Result<T, Error>;
