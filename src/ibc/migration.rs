use super::{IbcV0, IbcV1};
use crate::{migrate::MigrateFrom, state::State};

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
