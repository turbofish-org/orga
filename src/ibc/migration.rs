use super::{IbcV0, IbcV1, IbcV2};
use crate::migrate::MigrateFrom;

impl MigrateFrom<IbcV0> for IbcV1 {
    fn migrate_from(_value: IbcV0) -> crate::Result<Self> {
        unreachable!()
    }
}

impl MigrateFrom<IbcV1> for IbcV2 {
    fn migrate_from(_value: IbcV1) -> crate::Result<Self> {
        todo!()
    }
}
