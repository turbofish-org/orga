use ibc::core::ics24_host::path::Path;
use ics23::LeafOp;
use tendermint_proto::v0_34::abci::{RequestQuery, ResponseQuery};
use tendermint_proto::v0_34::crypto::{ProofOp, ProofOps};

use super::{Ibc, IBC_QUERY_PATH};
use crate::abci::AbciQuery;
use crate::store::Read;
use crate::{Error, Result};

impl AbciQuery for Ibc {
    fn abci_query(&self, req: &RequestQuery) -> Result<ResponseQuery> {
        if req.path != IBC_QUERY_PATH {
            return Err(Error::Ibc("Invalid query path".to_string()));
        }
        let data = req.data.to_vec();

        let path: Path = String::from_utf8(data.clone())
            .map_err(|_| Error::Ibc("Invalid query data encoding".to_string()))?
            .parse()
            .map_err(|_| Error::Ibc("Invalid query data".to_string()))?;

        let value_bytes = self.store.get(&data)?.unwrap_or_default();
        let key = path.clone().into_bytes();

        use prost::Message;

        let mut outer_proof_bytes = vec![];
        let inner_root_hash = self
            .store
            .backing_store()
            .borrow()
            .use_merkstore(|store| store.merk().root_hash());

        let outer_proof = ics23::CommitmentProof {
            proof: Some(ics23::commitment_proof::Proof::Exist(
                ics23::ExistenceProof {
                    key: b"ibc".to_vec(),
                    value: inner_root_hash.to_vec(),
                    leaf: Some(LeafOp {
                        hash: 6,
                        length: 0,
                        prehash_key: 0,
                        prehash_value: 0,
                        prefix: vec![],
                    }),
                    path: vec![],
                },
            )),
        };
        outer_proof
            .encode(&mut outer_proof_bytes)
            .map_err(|_| Error::Ibc("Failed to create outer proof".into()))?;

        let mut proof_bytes = vec![];
        let proof = self
            .store
            .backing_store()
            .borrow()
            .use_merkstore(|store| store.create_ics23_proof(key.as_slice()))?;

        proof
            .encode(&mut proof_bytes)
            .map_err(|_| Error::Ibc("Failed to create proof".into()))?;

        Ok(ResponseQuery {
            code: 0,
            key: req.data.clone(),
            value: value_bytes.into(),
            proof_ops: Some(ProofOps {
                ops: vec![
                    ProofOp {
                        r#type: "".to_string(),
                        key: path.into_bytes(),
                        data: proof_bytes,
                    },
                    ProofOp {
                        r#type: "".to_string(),
                        key: b"ibc".to_vec(),
                        data: outer_proof_bytes,
                    },
                ],
            }),
            height: self.height as i64,
            ..Default::default()
        })
    }
}
