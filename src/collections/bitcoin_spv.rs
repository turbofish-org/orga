use super::Deque;
use crate::state::State;
use crate::store::DefaultBackingStore;
use crate::store::Store;
use crate::Result;
use bitcoin::consensus::{Decodable, Encodable};
use bitcoin::BlockHeader;
use ed::{Decode, Encode};
use std::io::{Read, Write};

pub struct SPVBlockHeader {
    inner: BlockHeader,
}

impl Encode for SPVBlockHeader {
    fn encode(&self) -> ed::Result<Vec<u8>> {
        let mut dest: Vec<u8> = Vec::new();
        self.encode_into(&mut dest)?;
        Ok(dest)
    }

    fn encode_into<W: Write>(&self, mut dest: &mut W) -> ed::Result<()> {
        let mut dest: Vec<u8> = Vec::new();
        match self.inner.consensus_encode(dest) {
            Ok(_) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    fn encoding_length(&self) -> ed::Result<usize> {
        let mut _dest: Vec<u8> = Vec::new();
        match self.inner.consensus_encode(_dest) {
            Ok(inner) => Ok(inner),
            Err(e) => Err(e.into()),
        }
    }
}

impl Decode for SPVBlockHeader {
    fn decode<R: Read>(input: R) -> ed::Result<Self> {
        let decoded_bytes: std::result::Result<BlockHeader, bitcoin::consensus::encode::Error> =
            Decodable::consensus_decode(input);
        match decoded_bytes {
            Ok(header) => Ok(Self { inner: header }),
            Err(e) => {
                let std_e =
                    std::io::Error::new(std::io::ErrorKind::Other, "Failed to decode header");
                Err(std_e.into())
            }
        }
    }
}

pub struct BitcoinSPV {
    deque: Deque<SPVBlockHeader>,
}

impl From<BitcoinSPV> for () {
    fn from(_spv: BitcoinSPV) {}
}

impl State<DefaultBackingStore> for BitcoinSPV {
    type Encoding = ();

    fn create(store: Store<DefaultBackingStore>, data: Self::Encoding) -> Result<Self> {
        Ok(BitcoinSPV {
            deque: Deque::create(store, Default::default())?,
        })
    }

    fn flush(self) -> Result<Self::Encoding> {
        Ok(())
    }
}

impl BitcoinSPV {}
