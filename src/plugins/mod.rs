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

pub mod chain_commitment;
pub use chain_commitment::{ChainCommitmentPlugin, ChainId};

pub mod sdk_compat;
pub use sdk_compat::{ConvertSdkTx, SdkCompatPlugin};

macro_rules! type_chain {
    ($name:tt<$($pfx_params:ident,)* _ $(,$sfx_params:ident)*>, $($tail:tt)*) => {
        $name<$($pfx_params,)* type_chain!($($tail)*), $($sfx_params),*>
    };

    ($name:tt) => {
        $name
    };
}

pub type DefaultPlugins<S, T, const ID: &'static str> = type_chain! {
    SdkCompatPlugin<S, _>,
    SignerPlugin<_>,
    ChainCommitmentPlugin<_, ID>,
    NoncePlugin<_>,
    PayablePlugin<_>,
    FeePlugin<S, _>,
    T
};
