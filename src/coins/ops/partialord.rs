use super::super::{Amount, Decimal, Ratio};
use std::cmp::{Ordering, PartialOrd};

impl PartialOrd<Amount> for Amount {
    fn partial_cmp(&self, other: &Amount) -> Option<Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl PartialOrd<Ratio> for Amount {
    fn partial_cmp(&self, other: &Ratio) -> Option<Ordering> {
        let self_ratio: Ratio = (*self).into();

        self_ratio.0.partial_cmp(&other.0)
    }
}

impl PartialOrd<Amount> for Ratio {
    fn partial_cmp(&self, other: &Amount) -> Option<Ordering> {
        let other_ratio: Ratio = (*other).into();

        self.0.partial_cmp(&other_ratio.0)
    }
}

impl PartialOrd<Ratio> for Ratio {
    fn partial_cmp(&self, other: &Ratio) -> Option<Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl PartialOrd<Amount> for u64 {
    fn partial_cmp(&self, other: &Amount) -> Option<Ordering> {
        self.partial_cmp(&other.0)
    }
}

impl PartialOrd<Ratio> for u64 {
    fn partial_cmp(&self, other: &Ratio) -> Option<Ordering> {
        let self_ratio: Ratio = (*self).into();
        self_ratio.partial_cmp(other)
    }
}

impl PartialOrd<u64> for Amount {
    fn partial_cmp(&self, other: &u64) -> Option<Ordering> {
        self.0.partial_cmp(other)
    }
}

impl PartialOrd<u64> for Ratio {
    fn partial_cmp(&self, other: &u64) -> Option<Ordering> {
        let other_ratio: Ratio = (*other).into();
        self.partial_cmp(&other_ratio)
    }
}

impl PartialOrd<Decimal> for Decimal {
    fn partial_cmp(&self, other: &Decimal) -> Option<Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl PartialOrd<u64> for Decimal {
    fn partial_cmp(&self, other: &u64) -> Option<Ordering> {
        let other_dec: Decimal = (*other).into();
        self.0.partial_cmp(&other_dec.0)
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

        self.0.partial_cmp(&other_decimal.0)
    }
}
