use super::super::{Amount, Ratio};
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
