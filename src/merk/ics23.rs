//! ICS-23 proofs from Merk.

use super::MerkStore;
use crate::{Error, Result};
use ics23::{
    commitment_proof::Proof, CommitmentProof, ExistenceProof, HashOp, InnerOp, InnerSpec, LeafOp,
    LengthOp, NonExistenceProof, ProofSpec,
};
use merk::tree::{Fetch, RefWalker, Tree};

/// Create an [ics23] proof for the provided key.
pub fn create_ics23_proof<S>(
    maybe_root: Option<RefWalker<'_, S>>,
    key: &[u8],
) -> Result<CommitmentProof>
where
    S: Fetch + Sized + Clone + Send,
{
    let root = maybe_root.ok_or_else(|| {
        Error::Merk(merk::Error::Proof(
            "Cannot create ICS 23 proof for empty tree".to_string(),
        ))
    })?;

    let proof = create_proof(root, key, vec![], None, None)?;
    Ok(CommitmentProof { proof: Some(proof) })
}

impl MerkStore {
    /// The proof specification used by this store.
    ///
    /// See:
    /// - [Merk algorithms](https://github.com/turbofish-org/merk/blob/develop/docs/algorithms.md)
    /// - [ProofSpec]
    pub fn ics23_spec() -> ProofSpec {
        ProofSpec {
            leaf_spec: Some(leaf_op()),
            inner_spec: Some(InnerSpec {
                child_order: vec![0, 1, 2],
                child_size: 32,
                empty_child: vec![0; 32],
                hash: HashOp::Sha512256.into(),
                max_prefix_length: 1,
                min_prefix_length: 1,
            }),
            max_depth: 0,
            min_depth: 0,
            prehash_key_before_comparison: false,
        }
    }
}

fn create_proof<S>(
    mut node: RefWalker<'_, S>,
    key: &[u8],
    mut path: Vec<InnerOp>,
    mut left_neighbor: Option<ExistenceProof>,
    mut right_neighbor: Option<ExistenceProof>,
) -> Result<Proof>
where
    S: Fetch + Sized + Clone + Send,
{
    let existence_proof = |mut path: Vec<InnerOp>, tree: &Tree| {
        path.reverse();
        ExistenceProof {
            key: tree.key().to_vec(),
            value: tree.value().to_vec(),
            leaf: Some(leaf_op()),
            path,
        }
    };

    if key == node.tree().key() {
        path.push(inner_op(&node, Branch::KV));
        let proof = existence_proof(path, node.tree());
        return Ok(Proof::Exist(proof));
    }

    let left = key < node.tree().key();

    // TODO: cloning every iteration is expensive, we should just track the
    // path index, key, value, and kv innerop
    let mut neighbor_path = path.clone();
    neighbor_path.push(inner_op(&node, Branch::KV));
    let neighbor_proof = existence_proof(neighbor_path, node.tree());
    if left {
        right_neighbor = Some(neighbor_proof);
    } else {
        left_neighbor = Some(neighbor_proof);
    }

    let left_op = inner_op(&node, Branch::Left);
    let right_op = inner_op(&node, Branch::Right);
    let maybe_child = node.walk(left)?;

    if let Some(child) = maybe_child {
        path.push(if left { left_op } else { right_op });
        return create_proof(child, key, path, left_neighbor, right_neighbor);
    }

    let proof = NonExistenceProof {
        key: key.to_vec(),
        left: left_neighbor,
        right: right_neighbor,
    };
    Ok(Proof::Nonexist(proof))
}

enum Branch {
    Left,
    Right,
    KV,
}

fn inner_op<S>(node: &RefWalker<'_, S>, branch: Branch) -> InnerOp
where
    S: Fetch + Sized + Clone + Send,
{
    let tree = node.tree();
    let kv_hash = || tree.kv_hash().to_vec();
    let left_hash = || tree.child_hash(true).to_vec();
    let right_hash = || tree.child_hash(false).to_vec();

    let concat = |a, b| [a, b].concat();

    let (prefix, suffix) = match branch {
        Branch::Left => (vec![], concat(kv_hash(), right_hash())),
        Branch::KV => (left_hash(), right_hash()),
        Branch::Right => (concat(left_hash(), kv_hash()), vec![]),
    };

    InnerOp {
        hash: HashOp::Sha512256.into(),
        prefix: concat(vec![1], prefix),
        suffix,
    }
}

fn leaf_op() -> LeafOp {
    LeafOp {
        hash: HashOp::Sha512256.into(),
        length: LengthOp::Fixed32Little.into(),
        prefix: vec![0],
        prehash_key: HashOp::NoHash.into(),
        prehash_value: HashOp::NoHash.into(),
    }
}

#[cfg(test)]
mod tests {
    use ics23::HostFunctionsManager;

    use crate::merk::ics23::create_ics23_proof;
    use crate::merk::MerkStore;
    use crate::store::Write;

    #[test]
    fn existence_proof() {
        let path = "/tmp/ics23-proof-test";
        let mut store = MerkStore::new(path);

        store.put(b"foo".to_vec(), b"1".to_vec()).unwrap();
        store.put(b"bar".to_vec(), b"2".to_vec()).unwrap();
        store.put(b"baz".to_vec(), b"3".to_vec()).unwrap();
        store.put(b"bar2".to_vec(), b"4".to_vec()).unwrap();
        store.put(b"baz2".to_vec(), b"5".to_vec()).unwrap();
        store.put(b"bar3".to_vec(), b"6".to_vec()).unwrap();
        store.put(b"baz4".to_vec(), b"7".to_vec()).unwrap();
        store.write(vec![]).unwrap();

        let proof = store
            .merk()
            .walk(|w| create_ics23_proof(w, b"foo").unwrap());
        let root_hash = store.merk().root_hash().to_vec();

        drop(store);
        merk::Merk::destroy(merk::Merk::open(path).unwrap()).unwrap();

        assert!(ics23::verify_membership::<HostFunctionsManager>(
            &proof,
            &MerkStore::ics23_spec(),
            &root_hash,
            b"foo",
            b"1"
        ));
    }

    #[ignore]
    #[test]
    fn nonexistence_proof() {
        let path = "/tmp/ics23-proof-test2";
        let mut store = MerkStore::new(path);

        store.put(b"foo".to_vec(), b"1".to_vec()).unwrap();
        store.put(b"bar".to_vec(), b"2".to_vec()).unwrap();
        store.put(b"baz".to_vec(), b"3".to_vec()).unwrap();
        store.put(b"bar2".to_vec(), b"4".to_vec()).unwrap();
        store.put(b"baz2".to_vec(), b"5".to_vec()).unwrap();
        store.put(b"bar3".to_vec(), b"6".to_vec()).unwrap();
        store.put(b"baz4".to_vec(), b"7".to_vec()).unwrap();
        store.write(vec![]).unwrap();

        let proof = store
            .merk()
            .walk(|w| create_ics23_proof(w, b"foo2").unwrap());
        dbg!(&proof);
        let root_hash = store.merk().root_hash().to_vec();

        drop(store);
        merk::Merk::destroy(merk::Merk::open(path).unwrap()).unwrap();

        assert!(ics23::verify_non_membership::<HostFunctionsManager>(
            &proof,
            &MerkStore::ics23_spec(),
            &root_hash,
            b"foo2",
        ));
    }
}
