#![feature(test)]

extern crate test;

use test::Bencher;
use orga::{WriteCache, Read, Write};

#[bench]
fn writecache_get_8b(b: &mut Bencher) {
    let store = WriteCache::new();

    let mut i: u32 = 0;
    b.iter(|| {
        store.get(&[0, 0, 0, 0, 0, 0, 0, (i % 2) as u8]).unwrap();
        i += 1;
    });
}

#[bench]
fn writecache_put_8b_2keys(b: &mut Bencher) {
    let mut store = WriteCache::new();

    let mut i: u32 = 0;
    b.iter(|| {
        store.put(
            vec![0, 0, 0, 0, 0, 0, 0, (i % 2) as u8],
            vec![0; 8]
        ).unwrap();
        i += 1;
    });
}

#[bench]
fn writecache_put_8b_256keys(b: &mut Bencher) {
    let mut store = WriteCache::new();

    let mut i: u32 = 0;
    b.iter(|| {
        store.put(
            vec![0, 0, 0, 0, 0, 0, 0, (i % 256) as u8],
            vec![0; 8]
        ).unwrap();
        i += 1;
    });
}

#[bench]
fn writecache_put_8b_65536keys(b: &mut Bencher) {
    let mut store = WriteCache::new();

    let mut i: u32 = 0;
    b.iter(|| {
        store.put(
            vec![0, 0, 0, 0, 0, 0, ((i >> 8) % 256) as u8, (i % 256) as u8],
            vec![0; 8]
        ).unwrap();
        i += 1;
    });
}


#[bench]
fn writecache_delete_8b(b: &mut Bencher) {
    let mut store = WriteCache::new();

    let mut i: u32 = 0;
    b.iter(|| {
        store.delete(&[0, 0, 0, 0, 0, 0, 0, (i % 2) as u8]).unwrap();
        i += 1;
    });
}
