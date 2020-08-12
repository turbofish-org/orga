use failure::Error;

/// A result type bound to the standard orga error type.    
pub type Result<T> = std::result::Result<T, Error>;
