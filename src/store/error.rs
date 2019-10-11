use error_chain::error_chain;

error_chain! {
    errors {
        NotFound {
            description("Key not found")
            display("Key not found")
        }
    }
}
