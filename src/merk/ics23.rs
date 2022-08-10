use super::MerkStore;
use crate::{Error, Result};
use ics23::{
    commitment_proof::Proof, CommitmentProof, ExistenceProof, HashOp, InnerOp, InnerSpec, LeafOp,
    LengthOp, NonExistenceProof, ProofSpec,
};
use merk::{
    tree::{RefWalker, Tree},
    MerkSource,
};

impl MerkStore {
    pub fn create_ics23_proof(&self, key: &[u8]) -> Result<CommitmentProof> {
        self.merk().walk(|maybe_root| {
            let root = maybe_root.ok_or_else(|| {
                Error::Merk(merk::Error::Proof(
                    "Cannot create ICS 23 proof for empty tree".to_string(),
                ))
            })?;

            let proof = create_proof(root, key, vec![], None, None)?;
            Ok(CommitmentProof { proof: Some(proof) })
        })
    }

    pub fn ics23_spec() -> ProofSpec {
        ProofSpec {
            leaf_spec: Some(leaf_op()),
            inner_spec: Some(InnerSpec {
                child_order: vec![1, 0, 2],
                child_size: 32,
                empty_child: vec![0; 32],
                hash: HashOp::Sha512256.into(),
                max_prefix_length: 1,
                min_prefix_length: 1,
            }),
            max_depth: 0,
            min_depth: 0,
        }
    }
}

fn create_proof<'a>(
    mut node: RefWalker<'a, MerkSource<'a>>,
    key: &[u8],
    mut path: Vec<InnerOp>,
    mut left_neighbor: Option<ExistenceProof>,
    mut right_neighbor: Option<ExistenceProof>,
) -> Result<Proof> {
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

fn inner_op<'a>(node: &RefWalker<MerkSource<'a>>, branch: Branch) -> InnerOp {
    let tree = node.tree();
    let kv_hash = || tree.kv_hash().to_vec();
    let left_hash = || tree.child_hash(true).to_vec();
    let right_hash = || tree.child_hash(false).to_vec();

    let concat = |a, b| [a, b].concat();

    let (prefix, suffix) = match branch {
        Branch::KV => (vec![], concat(left_hash(), right_hash())),
        Branch::Left => (kv_hash(), right_hash()),
        Branch::Right => (concat(kv_hash(), left_hash()), vec![]),
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
    use crate::merk::MerkStore;
    use crate::store::Write;

    #[test]
    fn existence_proof() {
        let path = "/tmp/ics23-proof-test";
        let mut store = MerkStore::new(path.into());

        store.put(b"foo".to_vec(), b"1".to_vec()).unwrap();
        store.put(b"bar".to_vec(), b"2".to_vec()).unwrap();
        store.put(b"baz".to_vec(), b"3".to_vec()).unwrap();
        store.put(b"bar2".to_vec(), b"4".to_vec()).unwrap();
        store.put(b"baz2".to_vec(), b"5".to_vec()).unwrap();
        store.put(b"bar3".to_vec(), b"6".to_vec()).unwrap();
        store.put(b"baz4".to_vec(), b"7".to_vec()).unwrap();
        store.write(vec![]).unwrap();

        let proof = store.create_ics23_proof(b"foo").unwrap();
        let root_hash = store.merk().root_hash().to_vec();

        drop(store);
        merk::Merk::destroy(merk::Merk::open(path).unwrap()).unwrap();

        assert!(ics23::verify_membership(
            &proof,
            &MerkStore::ics23_spec(),
            &root_hash,
            b"foo",
            b"1"
        ));
    }

    #[ignore]
    #[test]
    #[ignore]
    fn nonexistence_proof() {
        let path = "/tmp/ics23-proof-test2";
        let mut store = MerkStore::new(path.into());

        store.put(b"foo".to_vec(), b"1".to_vec()).unwrap();
        store.put(b"bar".to_vec(), b"2".to_vec()).unwrap();
        store.put(b"baz".to_vec(), b"3".to_vec()).unwrap();
        store.put(b"bar2".to_vec(), b"4".to_vec()).unwrap();
        store.put(b"baz2".to_vec(), b"5".to_vec()).unwrap();
        store.put(b"bar3".to_vec(), b"6".to_vec()).unwrap();
        store.put(b"baz4".to_vec(), b"7".to_vec()).unwrap();
        store.write(vec![]).unwrap();

        let proof = store.create_ics23_proof(b"foo2").unwrap();
        dbg!(&proof);
        let root_hash = store.merk().root_hash().to_vec();

        drop(store);
        merk::Merk::destroy(merk::Merk::open(path).unwrap()).unwrap();

        assert!(ics23::verify_non_membership(
            &proof,
            &MerkStore::ics23_spec(),
            &root_hash,
            b"foo2",
        ));
    }
}
