use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("ABCI Error: {0}")]
    ABCI(String),
    #[error("Call Error: {0}")]
    Call(String),
    #[error("Client Error: {0}")]
    Client(String),
    #[error(transparent)]
    Dalek(#[from] ed25519_dalek::ed25519::Error),
    #[error("Downcast Error: {0}")]
    Downcast(String),
    #[error(transparent)]
    Ed(#[from] ed::Error),
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    Merk(#[from] merk::Error),
    #[error("Nonce Error: {0}")]
    Nonce(String),
    #[error("Tendermint Error: {0}")]
    Tendermint(String),
    #[error("Signer Error: {0}")]
    Signer(String),
    #[error("Store Error: {0}")]
    Store(String),
    #[error("Test Error: {0}")]
    Test(String),
    #[error("Query Error: {0}")]
    Query(String),
    #[error("Unknown Error")]
    Unknown,
}

/// A result type bound to the standard orga error type.    
pub type Result<T> = std::result::Result<T, Error>;
