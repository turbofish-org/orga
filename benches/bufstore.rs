#![feature(test)]

extern crate test;

use orga::store::{MapStore, Read, Write};
use test::Bencher;

#[bench]
fn bufstore_get_8b(b: &mut Bencher) {
    let store = MapStore::new();

    let mut i: u32 = 0;
    b.iter(|| {
        store.get(&[0, 0, 0, 0, 0, 0, 0, (i % 2) as u8]).unwrap();
        i += 1;
    });
}

#[bench]
fn bufstore_put_8b_2keys(b: &mut Bencher) {
    let mut store = MapStore::new();

    let mut i: u32 = 0;
    b.iter(|| {
        store
            .put(vec![0, 0, 0, 0, 0, 0, 0, (i % 2) as u8], vec![0; 8])
            .unwrap();
        i += 1;
    });
}

#[bench]
fn bufstore_put_8b_256keys(b: &mut Bencher) {
    let mut store = MapStore::new();

    let mut i: u32 = 0;
    b.iter(|| {
        store
            .put(vec![0, 0, 0, 0, 0, 0, 0, (i % 256) as u8], vec![0; 8])
            .unwrap();
        i += 1;
    });
}

#[bench]
fn bufstore_put_8b_65536keys(b: &mut Bencher) {
    let mut store = MapStore::new();

    let mut i: u32 = 0;
    b.iter(|| {
        store
            .put(
                vec![0, 0, 0, 0, 0, 0, ((i >> 8) % 256) as u8, (i % 256) as u8],
                vec![0; 8],
            )
            .unwrap();
        i += 1;
    });
}

#[bench]
fn bufstore_delete_8b(b: &mut Bencher) {
    let mut store = MapStore::new();

    let mut i: u32 = 0;
    b.iter(|| {
        store.delete(&[0, 0, 0, 0, 0, 0, 0, (i % 2) as u8]).unwrap();
        i += 1;
    });
}
