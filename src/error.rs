use error_chain::error_chain;
use crate::Store;

error_chain! {
    foreign_links {
        Store(crate::store::Error);
        Abci(abci2::Error);
    }
}
