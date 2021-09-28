use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("ABCI Error: {0}")]
    ABCI(String),
    #[error("Nonce Error: {0}")]
    Nonce(String),
    #[error("Signer Error: {0}")]
    Signer(String),
    #[error("Store Error: {0}")]
    Store(String),
    #[error("Unknown Error")]
    Unknown,
}

/// A result type bound to the standard orga error type.    
pub type Result<T> = std::result::Result<T, Error>;
