use crate::state::State;
use crate::Result;

pub trait Migrate {
    type Legacy: orgav1::state::State;

    fn migrate(&mut self, legacy: Self::Legacy) -> Result<()>;
}
