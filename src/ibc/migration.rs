use super::{IbcV0, IbcV1, Timestamp};
use crate::migrate::MigrateFrom;

impl MigrateFrom<IbcV0> for IbcV1 {
    fn migrate_from(_value: IbcV0) -> crate::Result<Self> {
        unreachable!()
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
