use super::super::{Amount, Decimal};
use std::cmp::PartialEq;

impl PartialEq<Amount> for Amount {
    fn eq(&self, other: &Amount) -> bool {
        self.value == other.value
    }
}

impl PartialEq<Amount> for u64 {
    fn eq(&self, other: &Amount) -> bool {
        *self == other.value
    }
}

impl PartialEq<u64> for Amount {
    fn eq(&self, other: &u64) -> bool {
        self.value == *other
    }
}

impl PartialEq<Decimal> for Decimal {
    fn eq(&self, other: &Decimal) -> bool {
        self.value == other.value
    }
}

impl PartialEq<u64> for Decimal {
    fn eq(&self, other: &u64) -> bool {
        let other_dec: Decimal = (*other).into();
        self.value == other_dec.value
    }
}

impl PartialEq<Decimal> for Amount {
    fn eq(&self, other: &Decimal) -> bool {
        let self_decimal: Decimal = (*self).into();

        self_decimal.value == other.value
    }
}

impl PartialEq<Amount> for Decimal {
    fn eq(&self, other: &Amount) -> bool {
        let other_decimal: Decimal = (*other).into();

        self.value == other_decimal.value
    }
}
