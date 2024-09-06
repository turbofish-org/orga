//! A store backed by Merkle proofs.
use crate::error::{Error, Result};
use crate::store::*;
use merk::proofs::query::Map as ProofMap;
use std::ops::Bound;

/// A store backed by a [ProofMap], allowing the loading of [State] types from
/// partial proofs.
///
/// The read operations of this store may produce errors due to missing data in
/// the underlying proof, in which case the consumer may fetch the missing data,
/// add it to the [ProofMap], and retry the operation.
///
/// [State]: crate::state::State
pub struct ProofStore(pub ProofMap);

impl Read for ProofStore {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let maybe_value = self.0.get(key).map_err(|err| {
            if let merk::Error::MissingData = err {
                Error::StoreErr(crate::store::Error::GetUnknown(key.to_vec()))
            } else {
                Error::Merk(err)
            }
        })?;
        Ok(maybe_value.map(|value| value.to_vec()))
    }

    fn get_next(&self, key: &[u8]) -> Result<Option<KV>> {
        let mut iter = self.0.range((Bound::Excluded(key), Bound::Unbounded));
        let item = iter.next().transpose().map_err(|err| {
            if let merk::Error::MissingData = err {
                Error::StoreErr(crate::store::Error::GetNextUnknown(key.to_vec()))
            } else {
                Error::Merk(err)
            }
        })?;
        Ok(item.map(|(k, v)| (k.to_vec(), v.to_vec())))
    }

    fn get_prev(&self, key: Option<&[u8]>) -> Result<Option<KV>> {
        let mut iter = self.0.range((
            Bound::Unbounded,
            key.map_or(Bound::Unbounded, Bound::Excluded),
        ));
        let item = iter.next_back().transpose()?;
        Ok(item.map(|(k, v)| (k.to_vec(), v.to_vec())))
    }
}
