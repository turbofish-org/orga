use super::Amount;
use crate::encoding::{Decode, Encode};
use crate::state::State;
use crate::store::Store;
use crate::{Error, Result};
use num_rational::Ratio as NumRatio;
use std::cmp::Ordering;
use std::convert::TryFrom;
use std::ops::{Deref, DerefMut};

#[derive(Clone, Copy, Debug)]
pub struct Ratio(pub(crate) NumRatio<u64>);

impl Encode for Ratio {
    fn encode_into<W: std::io::Write>(&self, dest: &mut W) -> ed::Result<()> {
        dest.write_all(self.0.numer().encode()?.as_slice())?;
        dest.write_all(self.0.denom().encode()?.as_slice())?;

        Ok(())
    }

    fn encoding_length(&self) -> ed::Result<usize> {
        Ok(8 * 2)
    }
}

impl Decode for Ratio {
    fn decode<R: std::io::Read>(mut source: R) -> ed::Result<Self> {
        let mut numer_bytes = [0u8; 8];
        let mut denom_bytes = [0u8; 8];
        source.read_exact(&mut numer_bytes)?;
        source.read_exact(&mut denom_bytes)?;
        let numer = u64::decode(numer_bytes.as_ref())?;
        let denom = u64::decode(numer_bytes.as_ref())?;
        Ratio::new(numer, denom).map_err(|_| ed::Error::UnexpectedByte(0))
    }
}

impl ed::Terminated for Ratio {}

impl Default for Ratio {
    fn default() -> Self {
        0.into()
    }
}

impl Deref for Ratio {
    type Target = NumRatio<u64>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Ratio {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<u64> for Ratio {
    fn from(value: u64) -> Self {
        Ratio(NumRatio::new(value, 1))
    }
}

impl From<NumRatio<u64>> for Ratio {
    fn from(value: NumRatio<u64>) -> Self {
        Ratio(value)
    }
}

impl Eq for Ratio {}

impl Ord for Ratio {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl Ratio {
    pub fn new(numer: u64, denom: u64) -> Result<Self> {
        if denom == 0 {
            return Err(Error::DivideByZero); // TODO: use another variant
        }

        Ok(Self(NumRatio::new(numer, denom)))
    }

    pub fn amount(&self) -> Amount {
        Amount::new(self.0.to_integer())
    }
}

#[derive(Encode, Decode)]
pub struct RatioEncoding {
    numerator: u64,
    denominator: u64,
}
impl Default for RatioEncoding {
    fn default() -> Self {
        Self {
            numerator: 0,
            denominator: 1,
        }
    }
}

impl State for Ratio {
    type Encoding = RatioEncoding;
    fn create(_store: Store, data: Self::Encoding) -> Result<Self> {
        Ratio::new(data.numerator, data.denominator)
    }

    fn flush(self) -> Result<Self::Encoding> {
        Ok(RatioEncoding {
            numerator: *(self.0).numer(),
            denominator: *(self.0).denom(),
        })
    }
}

impl From<Ratio> for RatioEncoding {
    fn from(ratio: Ratio) -> Self {
        RatioEncoding {
            numerator: *(ratio.0).numer(),
            denominator: *(ratio.0).denom(),
        }
    }
}

impl TryFrom<Result<Ratio>> for Ratio {
    type Error = Error;

    fn try_from(value: Result<Ratio>) -> Result<Self> {
        value
    }
}

impl From<Amount> for Ratio {
    fn from(amount: Amount) -> Self {
        Self::new(amount.0, 1).unwrap()
    }
}
