mod signer;
pub use signer::*;

mod nonce;
pub use nonce::*;

mod abci;
pub use abci::*;

mod payable;
pub use payable::*;

mod fee;
pub use fee::*;

pub mod ibc;

pub mod chain_commitment;
pub use chain_commitment::{ChainCommitmentPlugin, ChainId};

pub mod sdk_compat;
pub use sdk_compat::{ConvertSdkTx, SdkCompatPlugin};

pub type DefaultPlugins<S, T, const ID: &'static str> = SdkCompatPlugin<
    S,
    SignerPlugin<ChainCommitmentPlugin<NoncePlugin<PayablePlugin<FeePlugin<S, T>>>, ID>>,
>;

// TODO: make a macro that can make this more readable, e.g.:
// type_chain! {
//     SDKCompatPlugin<S, _, ID>,
//     SignerPlugin<_>,
//     ChainCommitmentPlugin<_, ID>,
//     NoncePlugin<_>,
//     PayablePlugin<_>,
//     FeePlugin<S, T>,
// }
