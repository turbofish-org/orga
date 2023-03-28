use super::{Commission, Declaration, Staking};
use crate::coins::{Address, Amount, Decimal, Give, Symbol};
use crate::encoding::Decode;
use crate::migrate::Migrate;
use crate::plugins::EndBlockCtx;
use crate::Result;
