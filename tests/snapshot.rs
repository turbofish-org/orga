#[test]
#[cfg(all(feature = "abci", feature = "merk/full", feature = "tendermint-proto"))]
fn drop_used_snapshot() {
    use orga::abci::ABCIStore;
    use orga::merk::{store::SNAPSHOT_INTERVAL, MerkStore};
    use orga::store::Write;
    use tempdir::TempDir;
    use tendermint_proto::v0_34::abci::RequestLoadSnapshotChunk;

    let dir = TempDir::new("test").unwrap().into_path();
    println!("snapshot test dir: {}", dir.display());

    let mut store = MerkStore::new(dir);

    for i in 0..10_000u32 {
        let key = i.to_be_bytes().to_vec();
        store.put(key, vec![123; 16]).unwrap();
    }

    store.commit(SNAPSHOT_INTERVAL).unwrap();

    let request_chunk = |height, chunk| RequestLoadSnapshotChunk {
        height,
        chunk,
        ..Default::default()
    };

    store
        .load_snapshot_chunk(request_chunk(SNAPSHOT_INTERVAL, 0))
        .unwrap();
}
