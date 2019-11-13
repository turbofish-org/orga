#![feature(test)]

extern crate test;

use test::Bencher;
use orga::{RWLog, NullStore, WriteCache, Read, Write};

#[bench]
fn rwlog_null_get_8b_2keys(b: &mut Bencher) {
    let store = RWLog::wrap(NullStore);

    let mut i: u32 = 0;
    b.iter(|| {
        store.get(&[0, 0, 0, 0, 0, 0, 0, (i % 2) as u8]).unwrap();
        i += 1;
    });
}

#[bench]
fn rwlog_null_get_8b_256keys(b: &mut Bencher) {
    let store = RWLog::wrap(NullStore);

    let mut i: u32 = 0;
    b.iter(|| {
        store.get(&[0, 0, 0, 0, 0, 0, 0, (i % 256) as u8]).unwrap();
        i += 1;
    });
}

#[bench]
fn rwlog_null_get_8b_65536keys(b: &mut Bencher) {
    let store = RWLog::wrap(NullStore);

    let mut i: u32 = 0;
    b.iter(|| {
        store.get(&[0, 0, 0, 0, 0, 0, ((i >> 8) % 256) as u8, (i % 256) as u8]).unwrap();
        i += 1;
    });
}

#[bench]
fn rwlog_null_put_8b_2keys(b: &mut Bencher) {
    let mut store = RWLog::wrap(NullStore);

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
fn rwlog_null_put_8b_256keys(b: &mut Bencher) {
    let mut store = RWLog::wrap(NullStore);

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
fn rwlog_null_put_8b_65536keys(b: &mut Bencher) {
    let mut store = RWLog::wrap(NullStore);

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
fn rwlog_null_delete_8b_2keys(b: &mut Bencher) {
    let mut store = RWLog::wrap(NullStore);

    let mut i: u32 = 0;
    b.iter(|| {
        store.delete(&[0, 0, 0, 0, 0, 0, 0, (i % 2) as u8]).unwrap();
        i += 1;
    });
}

#[bench]
fn rwlog_null_delete_8b_256keys(b: &mut Bencher) {
    let mut store = RWLog::wrap(NullStore);

    let mut i: u32 = 0;
    b.iter(|| {
        store.delete(&[0, 0, 0, 0, 0, 0, 0, (i % 256) as u8]).unwrap();
        i += 1;
    });
}

#[bench]
fn rwlog_null_delete_8b_65536keys(b: &mut Bencher) {
    let mut store = RWLog::wrap(NullStore);

    let mut i: u32 = 0;
    b.iter(|| {
        store.delete(&[0, 0, 0, 0, 0, 0, ((i >> 8) % 256) as u8, (i % 256) as u8]).unwrap();
        i += 1;
    });
}


#[bench]
fn rwlog_writecache_get_8b_2keys(b: &mut Bencher) {
    let store = RWLog::wrap(WriteCache::new());

    let mut i: u32 = 0;
    b.iter(|| {
        store.get(&[0, 0, 0, 0, 0, 0, 0, (i % 2) as u8]).unwrap();
        i += 1;
    });
}

#[bench]
fn rwlog_writecache_get_8b_256keys(b: &mut Bencher) {
    let store = RWLog::wrap(WriteCache::new());

    let mut i: u32 = 0;
    b.iter(|| {
        store.get(&[0, 0, 0, 0, 0, 0, 0, (i % 256) as u8]).unwrap();
        i += 1;
    });
}

#[bench]
fn rwlog_writecache_get_8b_65536keys(b: &mut Bencher) {
    let store = RWLog::wrap(WriteCache::new());

    let mut i: u32 = 0;
    b.iter(|| {
        store.get(&[0, 0, 0, 0, 0, 0, ((i >> 8) % 256) as u8, (i % 256) as u8]).unwrap();
        i += 1;
    });
}

#[bench]
fn rwlog_writecache_put_8b_2keys(b: &mut Bencher) {
    let mut store = RWLog::wrap(WriteCache::new());

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
fn rwlog_writecache_put_8b_256keys(b: &mut Bencher) {
    let mut store = RWLog::wrap(WriteCache::new());

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
fn rwlog_writecache_put_8b_65536keys(b: &mut Bencher) {
    let mut store = RWLog::wrap(WriteCache::new());

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
fn rwlog_writecache_delete_8b_2keys(b: &mut Bencher) {
    let mut store = RWLog::wrap(WriteCache::new());

    let mut i: u32 = 0;
    b.iter(|| {
        store.delete(&[0, 0, 0, 0, 0, 0, 0, (i % 2) as u8]).unwrap();
        i += 1;
    });
}

#[bench]
fn rwlog_writecache_delete_8b_256keys(b: &mut Bencher) {
    let mut store = RWLog::wrap(WriteCache::new());

    let mut i: u32 = 0;
    b.iter(|| {
        store.delete(&[0, 0, 0, 0, 0, 0, 0, (i % 256) as u8]).unwrap();
        i += 1;
    });
}

#[bench]
fn rwlog_writecache_delete_8b_65536keys(b: &mut Bencher) {
    let mut store = RWLog::wrap(WriteCache::new());

    let mut i: u32 = 0;
    b.iter(|| {
        store.delete(&[0, 0, 0, 0, 0, 0, ((i >> 8) % 256) as u8, (i % 256) as u8]).unwrap();
        i += 1;
    });
}
