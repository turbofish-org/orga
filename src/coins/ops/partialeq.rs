use super::super::{Amount, Ratio};
use std::cmp::PartialEq;

impl PartialEq<Amount> for Amount {
    fn eq(&self, other: &Amount) -> bool {
        self.0 == other.0
    }
}

impl PartialEq<Amount> for Ratio {
    fn eq(&self, other: &Amount) -> bool {
        let other_ratio: Ratio = (*other).into();

        self.0 == other_ratio.0
    }
}

impl PartialEq<Ratio> for Ratio {
    fn eq(&self, other: &Ratio) -> bool {
        self.0 == other.0
    }
}

impl PartialEq<Ratio> for Amount {
    fn eq(&self, other: &Ratio) -> bool {
        let self_ratio: Ratio = (*self).into();

        self_ratio.0 == other.0
    }
}

impl PartialEq<Amount> for u64 {
    fn eq(&self, other: &Amount) -> bool {
        *self == other.0
    }
}

impl PartialEq<Ratio> for u64 {
    fn eq(&self, other: &Ratio) -> bool {
        let self_ratio: Ratio = (*self).into();
        self_ratio.0 == other.0
    }
}

impl PartialEq<u64> for Amount {
    fn eq(&self, other: &u64) -> bool {
        self.0 == *other
    }
}

impl PartialEq<u64> for Ratio {
    fn eq(&self, other: &u64) -> bool {
        let other_ratio: Ratio = (*other).into();
        self.0 == other_ratio.0
    }
}
