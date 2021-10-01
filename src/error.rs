use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[cfg(feature = "abci")]
    #[error("ABCI Error: {0}")]
    ABCI(String),
    #[cfg(feature = "abci")]
    #[error(transparent)]
    ABCI2(#[from] abci2::Error),
    #[error("Call Error: {0}")]
    Call(String),
    #[error("Client Error: {0}")]
    Client(String),
    #[cfg(feature = "abci")]
    #[error(transparent)]
    Dalek(#[from] ed25519_dalek::ed25519::Error),
    #[error("Downcast Error: {0}")]
    Downcast(String),
    #[error(transparent)]
    Ed(#[from] ed::Error),
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[cfg(feature = "merk")]
    #[error(transparent)]
    Merk(#[from] merk::Error),
    #[error("Nonce Error: {0}")]
    Nonce(String),
    #[error("Tendermint Error: {0}")]
    Tendermint(String),
    #[cfg(feature = "merk")]
    #[error(transparent)]
    RocksDB(#[from] merk::rocksdb::Error),
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
