use std::collections::HashMap;
use error_chain::bail;
use crate::error::Result;
use crate::store::{WriteCache, Flush};
use super::{Store, Atomic};

// TODO: this is just a naive router implementation to figure things out as I
// go. we can make a much more performant one by e.g. using array indices rather
// than a hashmap of string keys (faster lookup and shorter prefixes), and/or
// static routing (maybe with macros?) rather than `dyn StateMachine`

trait Handler = Fn(&mut dyn Store, Vec<u8>) -> Result<()>;

#[derive(Default)]
pub struct Router {
    routes: HashMap<String, &'static dyn Handler>
}

pub struct Transaction {
    // TODO: use slices so we don't have to alloc/copy
    route: String,
    data: Vec<u8>
}

impl Router {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn route(mut self, name: String, sm: &'static Handler) -> Self {
        if self.routes.contains_key(&name) {
            panic!("A route for '{}' already exists", name);
        }
        self.routes.insert(name, sm);
        self
    }

    pub fn build(self) -> impl Fn(&mut dyn Store, Transaction) -> Result<()> {
        move |store, tx| {
            match self.routes.get(tx.route.as_str()) {
                Some(handler) => handler(store, tx.data),
                None => Err(format!("Route '{}' not found", tx.route).into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::store::WriteCache;
    use super::{Router, Transaction};

    #[test]
    fn router() {
        let mut store = WriteCache::new();

        let router = Router::new()
            .route("acoin".into(), &|store, tx| {
                println!("got acoin tx: {:?}", tx);
                Ok(())
            })
            .route("bcoin".into(), &|store, tx| {
                println!("got bcoin tx: {:?}", tx);
                Ok(())
            })
            .build();

        let tx = Transaction {
            route: "bcoin".into(),
            data: vec![1, 2, 3]
        };
        router(&mut store, tx).unwrap();
    }
}
