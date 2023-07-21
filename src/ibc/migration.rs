use super::{IbcV0, IbcV1, Timestamp};
use crate::{migrate::MigrateFrom, state::State};

/// Upgrade from ibc-rs v0.15 -> v0.40.
/// This migration resets all IBC state.
impl MigrateFrom<IbcV0> for IbcV1 {
    fn migrate_from(mut value: IbcV0) -> crate::Result<Self> {
        // TODO: implement actual IBC migration

        value
            .root_store
            .remove_range(b"a".to_vec()..b"z".to_vec())?;

        value.local_store.remove_range(..)?;

        let mut out = vec![];
        value.root_store.flush(&mut out)?;
        value.local_store.flush(&mut out)?;

        Ok(Self::default())
    }
}

impl MigrateFrom<i128> for Timestamp {
    fn migrate_from(other: i128) -> crate::Result<Self> {
        let nanos = other
            .try_into()
            .map_err(|_| crate::Error::Ibc("Invalid timestamp".to_string()))?;

        Ok(Self {
            inner: ibc::core::timestamp::Timestamp::from_nanoseconds(nanos)
                .map_err(|e| crate::Error::Ibc(e.to_string()))?,
        })
    }
}
