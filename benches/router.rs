// #![feature(test)]

// extern crate test;

// use test::Bencher;
// use orga::{Router, RouterTransaction, NullStore};

// #[bench]
// fn router_8(b: &mut Bencher) {
//     let mut router = Router::new();
//     let mut routes = vec![];
//     for i in 0..8 {
//         let route = format!("{}", i).to_string();
//         routes.push(route.clone());
//         router  = router.route(route, &|_, _| Ok(()));
//     }
//     let router = router.build();

//     let mut i = 0;
//     b.iter(|| {
//         let route = routes[i % 8].clone();
//         i += 1;
//         let tx = RouterTransaction { route, data: vec![1, 2, 3] };
//         router(&mut NullStore, tx).unwrap();
//     });
// }

// #[bench]
// fn router_128(b: &mut Bencher) {
//     let mut router = Router::new();
//     let mut routes = vec![];
//     for i in 0..128 {
//         let route = format!("{}", i).to_string();
//         routes.push(route.clone());
//         router  = router.route(route, &|_, _| Ok(()));
//     }
//     let router = router.build();

//     let mut i = 0;
//     b.iter(|| {
//         let route = routes[i % 128].clone();
//         i += 1;
//         let tx = RouterTransaction { route, data: vec![1, 2, 3] };
//         router(&mut NullStore, tx).unwrap();
//     });
// }
