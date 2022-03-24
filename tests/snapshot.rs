use orga::store::{Read, Write};
use orga::abci::ABCIStore;
use orga::merk::{MerkStore, store::SNAPSHOT_INTERVAL};
use tendermint_proto::abci::RequestLoadSnapshotChunk;
use tempdir::TempDir;

#[test]
fn drop_used_snapshot() {
    let dir = TempDir::new("test").unwrap().into_path();
    println!("snapshot test dir: {}", dir.display());

    let mut store = MerkStore::new(dir);

    for i in 0..10_000u32 {
        let key = i.to_be_bytes().to_vec();
        store.put(key, vec![123; 16]);
    }

    store.commit(SNAPSHOT_INTERVAL).unwrap();

    let request_chunk = |height, chunk| {
        let mut req: RequestLoadSnapshotChunk = Default::default();
        req.height = height;
        req.chunk = chunk;
        req
    };

    store.load_snapshot_chunk(request_chunk(SNAPSHOT_INTERVAL, 0)).unwrap();
}