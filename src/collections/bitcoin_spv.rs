use super::Deque;
use bitcoin::BlockHeader;

pub struct BitcoinSPV {
    deque: Deque<BlockHeader>,
}
