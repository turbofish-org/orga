use crate::state::State;
use crate::Result;

pub trait Migrate {
    type Legacy: State;

    fn migrate(&mut self, legacy: Self::Legacy) -> Result<()>;
}
