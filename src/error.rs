use error_chain::error_chain;

error_chain! {
    foreign_links {
        Store(crate::store::Error);
        Merk(merk::Error);
    }
}
