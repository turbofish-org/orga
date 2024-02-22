use super::super::{Amount, Decimal};
use std::cmp::{Ordering, PartialOrd};

impl PartialOrd<Amount> for u64 {
    fn partial_cmp(&self, other: &Amount) -> Option<Ordering> {
        self.partial_cmp(&other.value)
    }
}

impl PartialOrd<u64> for Amount {
    fn partial_cmp(&self, other: &u64) -> Option<Ordering> {
        self.value.partial_cmp(other)
    }
}

impl PartialOrd<u64> for Decimal {
    fn partial_cmp(&self, other: &u64) -> Option<Ordering> {
        let other_dec: Decimal = (*other).into();
        self.value.partial_cmp(&other_dec.value)
    }
}

impl PartialOrd<Decimal> for Amount {
    fn partial_cmp(&self, other: &Decimal) -> Option<Ordering> {
        let self_decimal: Decimal = (*self).into();

        self_decimal.partial_cmp(other)
    }
}

impl PartialOrd<Amount> for Decimal {
    fn partial_cmp(&self, other: &Amount) -> Option<Ordering> {
        let other_decimal: Decimal = (*other).into();

        self.value.partial_cmp(&other_decimal.value)
    }
}
