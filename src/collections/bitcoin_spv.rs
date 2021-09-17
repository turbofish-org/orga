use super::Deque;
use crate::state::State;
use crate::Result;
use bitcoin::consensus::{Decodable, Encodable};
use bitcoin::BlockHeader;
use ed::{Decode, Encode};
use std::io::{Read, Write};

pub struct SPVBlockHeader {
    inner: BlockHeader,
}

impl From<BlockHeader> for SPVBlockHeader {
    fn from(inner: BlockHeader) -> Self {
        Self { inner }
    }
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
        let decoded_bytes = Decodable::consensus_decode(input);
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

#[derive(State)]
pub struct BitcoinSPV {
    deque: Deque<SPVBlockHeader>,
}

impl BitcoinSPV {
    //this is just a sigle Header
    fn add(header: SPVBlockHeader) -> Result<()> {
        //this should either just take a BlockHeader
        //or I should implement From<BlockHeader> for SPVBlockHeader
        Ok(())
    }

    //this add a list of headers
    //wish I could extend this to a collection of headers as well, that would be nice
    fn add_all<const N: usize>(headers: [SPVBlockHeader; N]) -> Result<()> {
        Ok(())
    }

    //this maybe should be IntoIterator as the trait here
    fn add_iter<T: Iterator>(iter: T) -> Result<()> {
        Ok(())
    }

    //this might need to just return BlockHeader
    fn get_by_height(height: u32) -> Option<SPVBlockHeader> {
        None
    }

    //this might need to just return BlockHeader
    fn get_by_hash(hash: [u8; 32]) -> Option<SPVBlockHeader> {
        None
    }

    fn height() -> u32 {
        42
    }

    fn verify_headers<const N: usize>(headers: [SPVBlockHeader; N]) -> Result<()> {
        Ok(())
    }

    fn verify_header_collection<T: Iterator>(iter: T) -> Result<()> {
        Ok(())
    }
}
