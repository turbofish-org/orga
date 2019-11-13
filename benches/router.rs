#![feature(test)]

extern crate test;

use test::Bencher;
use orga::{Router, RouterTransaction, NullStore};

#[bench]
fn router_8(b: &mut Bencher) {
    let mut router = Router::new();
    let mut routes = vec![];
    for i in 0..8 {
        let route = String::from_utf8(vec![1, 1, 1, (i + 1) as u8]).unwrap();
        routes.push(route.clone());
        router  = router.route(route, &|store, tx| Ok(()));
    }
    let router = router.build();

    let mut i = 0;
    b.iter(|| {
        let route = routes[i % 8].clone();
        i += 1;
        let tx = RouterTransaction { route, data: vec![1, 2, 3] };
        router(&mut NullStore, tx).unwrap();
    });
}
