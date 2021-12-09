use crate::call::Call;
use crate::client::{AsyncCall, CallChain, Client};
use crate::coins::{Address, Amount, Coin, Give, Symbol, Take};
use crate::collections::Map;
use crate::context::GetContext;
use crate::encoding::{Decode, Encode};
use crate::plugins::Paid;
use crate::plugins::Signer;
use crate::query::Query;
use crate::state::State;
use crate::{Error, Result};

#[derive(State, Encode, Decode, Call, Query)]
pub struct Accounts<S: Symbol> {
    accounts: Map<Address, Coin<S>>,
}

impl<S: Symbol> Accounts<S> {
    #[call]
    pub fn transfer(&mut self, to: Address, amount: Amount) -> Result<()> {
        let taken_coins = self.take_own_coins(amount)?;
        let mut receiver = self.accounts.entry(to)?.or_insert_default()?;
        receiver.give(taken_coins)?;

        Ok(())
    }

    #[call]
    pub fn take_as_funding(&mut self, amount: Amount) -> Result<()> {
        let taken_coins = self.take_own_coins(amount)?;

        let paid = self
            .context::<Paid>()
            .ok_or_else(|| Error::Coins("No Paid context found".into()))?;

        paid.give::<S, _>(taken_coins.amount)
    }

    fn take_own_coins(&mut self, amount: Amount) -> Result<Coin<S>> {
        let signer = self.signer()?;

        let taken_coins = self
            .accounts
            .get_mut(signer)?
            .ok_or_else(|| Error::Coins("Insufficient funds".into()))?
            .take(amount)?;

        Ok(taken_coins)
    }

    fn signer(&mut self) -> Result<Address> {
        self.context::<Signer>()
            .ok_or_else(|| Error::Signer("No Signer context available".into()))?
            .signer
            .ok_or_else(|| Error::Coins("Unauthorized account action".into()))
    }

    #[call]
    pub fn give_from_funding(&mut self, amount: Amount) -> Result<()> {
        let taken_coins = self
            .context::<Paid>()
            .ok_or_else(|| Error::Coins("No Paid context found".into()))?
            .take(amount)?;

        self.give_own_coins(taken_coins)
    }

    fn give_own_coins(&mut self, coins: Coin<S>) -> Result<()> {
        let signer = self.signer()?;

        self.accounts
            .get_mut(signer)?
            .ok_or_else(|| Error::Coins("Insufficient funds".into()))?
            .give(coins)?;

        Ok(())
    }

    #[query]
    pub fn balance(&self, address: Address) -> Result<Amount> {
        match self.accounts.get(address)? {
            Some(coin) => Ok(coin.amount),
            None => Ok(0.into()),
        }
    }

    pub fn deposit(&mut self, address: Address, coins: Coin<S>) -> Result<()> {
        let mut account = self.accounts.entry(address)?.or_insert_default()?;
        account.give(coins)?;

        Ok(())
    }

    pub fn withdraw(&mut self, address: Address, amount: Amount) -> Result<Coin<S>> {
        let mut account = self.accounts.entry(address)?.or_insert_default()?;
        account.take(amount)
    }
}

pub mod accounts_client {
    use super::*;
    use std::marker::PhantomData;
    #[must_use]
    pub struct Client<S, __Parent>
    where
        S: Symbol,
        __Parent: Clone + Send,
    {
        pub(super) parent: __Parent,
        _marker: PhantomData<S>,
    }
    impl<S, __Parent> Clone for Client<S, __Parent>
    where
        S: Symbol,
        __Parent: Clone + Send,
    {
        fn clone(&self) -> Self {
            Self {
                parent: self.parent.clone(),
                _marker: PhantomData,
            }
        }
    }
    impl<S: Symbol, __Parent> Client<S, __Parent>
    where
        __Parent: Clone + Send,
    {
        pub fn new(parent: __Parent) -> Self {
            use ::orga::client::Client as _;
            Client {
                parent,
                _marker: PhantomData,
            }
        }
    }
    pub struct MethodTransferAdapter<S, __Return, __Parent>
    where
        S: Symbol,
        __Parent: Clone + Send,
        __Parent: ::orga::client::AsyncCall<Call = <Accounts<S> as ::orga::call::Call>::Call>,
    {
        pub(super) parent: __Parent,
        args: (Address, Amount),
        _marker: std::marker::PhantomData<(Accounts<S>, __Return)>,
    }
    unsafe impl<S: Symbol, __Parent, __Return> Send for MethodTransferAdapter<S, __Return, __Parent>
    where
        __Parent: Clone + Send,
        __Parent: ::orga::client::AsyncCall<Call = <Accounts<S> as ::orga::call::Call>::Call>,
    {
    }
    impl<S: Symbol, __Parent, __Return> Clone for MethodTransferAdapter<S, __Return, __Parent>
    where
        __Parent: Clone + Send,
        __Parent: ::orga::client::AsyncCall<Call = <Accounts<S> as ::orga::call::Call>::Call>,
    {
        fn clone(&self) -> Self {
            let encoded_args = ::orga::encoding::Encode::encode(&self.args).unwrap();
            let cloned_args = ::orga::encoding::Decode::decode(encoded_args.as_slice()).unwrap();
            Self {
                parent: self.parent.clone(),
                args: cloned_args,
                _marker: std::marker::PhantomData,
            }
        }
    }
    impl<S: Symbol, __Parent, __Return> ::orga::client::AsyncCall
        for MethodTransferAdapter<S, __Return, __Parent>
    where
        __Parent: Clone + Send,
        __Parent: ::orga::client::AsyncCall<Call = <Accounts<S> as ::orga::call::Call>::Call>,
        __Return: ::orga::call::Call,
        __Return::Call: Send + Sync,
    {
        type Call = <__Return as ::orga::call::Call>::Call;
        #[allow(
            clippy::let_unit_value,
            clippy::type_complexity,
            clippy::type_repetition_in_bounds,
            clippy::used_underscore_binding
        )]
        fn call<'life0, 'async_trait>(
            &'life0 mut self,
            call: Self::Call,
        ) -> ::core::pin::Pin<
            Box<
                dyn ::core::future::Future<Output = Result<()>>
                    + ::core::marker::Send
                    + 'async_trait,
            >,
        >
        where
            'life0: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(async move {
                if let ::core::option::Option::Some(__ret) =
                    ::core::option::Option::None::<Result<()>>
                {
                    return __ret;
                }
                let mut __self = self;
                let call = call;
                let __ret: Result<()> = {
                    let call_bytes = ::orga::encoding::Encode::encode(&call)?;
                    let parent_call = <Accounts<S> as ::orga::call::Call>::Call::MethodTransfer(
                        __self.args.0,
                        __self.args.1,
                        call_bytes,
                    );
                    __self.parent.call(parent_call).await
                };
                #[allow(unreachable_code)]
                __ret
            })
        }
    }
    impl<S: Symbol, __Parent> Client<S, __Parent>
    where
        __Parent: Clone + Send,
        __Parent: ::orga::client::AsyncCall<Call = <Accounts<S> as ::orga::call::Call>::Call>,
    {
        pub fn transfer(
            &mut self,
            to: Address,
            amount: Amount,
        ) -> ::orga::client::CallChain<
            <Result<()> as ::orga::client::Client<
                MethodTransferAdapter<S, Result<()>, __Parent>,
            >>::Client,
            MethodTransferAdapter<S, Result<()>, __Parent>,
        > {
            let adapter = MethodTransferAdapter {
                parent: self.parent.clone(),
                args: (to, amount),
                _marker: std::marker::PhantomData,
            };
            let client = <Result<()> as ::orga::client::Client<
                MethodTransferAdapter<S, _, __Parent>,
            >>::create_client(adapter.clone());
            ::orga::client::CallChain::new(client, adapter)
        }
    }
    pub struct MethodTakeAsFundingAdapter<S, __Return, __Parent>
    where
        S: Symbol,
        __Parent: Clone + Send,
        __Parent: ::orga::client::AsyncCall<Call = <Accounts<S> as ::orga::call::Call>::Call>,
    {
        pub(super) parent: __Parent,
        args: (Amount,),
        _marker: std::marker::PhantomData<(Accounts<S>, __Return)>,
    }
    unsafe impl<S: Symbol, __Parent, __Return> Send
        for MethodTakeAsFundingAdapter<S, __Return, __Parent>
    where
        __Parent: Clone + Send,
        __Parent: ::orga::client::AsyncCall<Call = <Accounts<S> as ::orga::call::Call>::Call>,
    {
    }
    impl<S: Symbol, __Parent, __Return> Clone for MethodTakeAsFundingAdapter<S, __Return, __Parent>
    where
        __Parent: Clone + Send,
        __Parent: ::orga::client::AsyncCall<Call = <Accounts<S> as ::orga::call::Call>::Call>,
    {
        fn clone(&self) -> Self {
            let encoded_args = ::orga::encoding::Encode::encode(&self.args).unwrap();
            let cloned_args = ::orga::encoding::Decode::decode(encoded_args.as_slice()).unwrap();
            Self {
                parent: self.parent.clone(),
                args: cloned_args,
                _marker: std::marker::PhantomData,
            }
        }
    }
    impl<S: Symbol, __Parent, __Return> ::orga::client::AsyncCall
        for MethodTakeAsFundingAdapter<S, __Return, __Parent>
    where
        __Parent: Clone + Send,
        __Parent: ::orga::client::AsyncCall<Call = <Accounts<S> as ::orga::call::Call>::Call>,
        __Return: ::orga::call::Call,
        __Return::Call: Send + Sync,
    {
        type Call = <__Return as ::orga::call::Call>::Call;
        #[allow(
            clippy::let_unit_value,
            clippy::type_complexity,
            clippy::type_repetition_in_bounds,
            clippy::used_underscore_binding
        )]
        fn call<'life0, 'async_trait>(
            &'life0 mut self,
            call: Self::Call,
        ) -> ::core::pin::Pin<
            Box<
                dyn ::core::future::Future<Output = Result<()>>
                    + ::core::marker::Send
                    + 'async_trait,
            >,
        >
        where
            'life0: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(async move {
                if let ::core::option::Option::Some(__ret) =
                    ::core::option::Option::None::<Result<()>>
                {
                    return __ret;
                }
                let mut __self = self;
                let call = call;
                let __ret: Result<()> = {
                    let call_bytes = ::orga::encoding::Encode::encode(&call)?;
                    let parent_call =
                        <Accounts<S> as ::orga::call::Call>::Call::MethodTakeAsFunding(
                            __self.args.0,
                            call_bytes,
                        );
                    __self.parent.call(parent_call).await
                };
                #[allow(unreachable_code)]
                __ret
            })
        }
    }
    impl<S: Symbol, __Parent> Client<S, __Parent>
    where
        __Parent: Clone + Send,
        __Parent: ::orga::client::AsyncCall<Call = <Accounts<S> as ::orga::call::Call>::Call>,
    {
        pub fn take_as_funding(
            &mut self,
            amount: Amount,
        ) -> ::orga::client::CallChain<
            <Result<()> as ::orga::client::Client<
                MethodTakeAsFundingAdapter<S, Result<()>, __Parent>,
            >>::Client,
            MethodTakeAsFundingAdapter<S, Result<()>, __Parent>,
        > {
            let adapter = MethodTakeAsFundingAdapter {
                parent: self.parent.clone(),
                args: (amount,),
                _marker: std::marker::PhantomData,
            };
            let client = <Result<()> as ::orga::client::Client<
                MethodTakeAsFundingAdapter<S, _, __Parent>,
            >>::create_client(adapter.clone());
            ::orga::client::CallChain::new(client, adapter)
        }
    }
    pub struct MethodGiveFromFundingAdapter<S, __Return, __Parent>
    where
        S: Symbol,
        __Parent: Clone + Send,
        __Parent: ::orga::client::AsyncCall<Call = <Accounts<S> as ::orga::call::Call>::Call>,
    {
        pub(super) parent: __Parent,
        args: (Amount,),
        _marker: std::marker::PhantomData<(Accounts<S>, __Return)>,
    }
    unsafe impl<S: Symbol, __Parent, __Return> Send
        for MethodGiveFromFundingAdapter<S, __Return, __Parent>
    where
        __Parent: Clone + Send,
        __Parent: ::orga::client::AsyncCall<Call = <Accounts<S> as ::orga::call::Call>::Call>,
    {
    }
    impl<S: Symbol, __Parent, __Return> Clone for MethodGiveFromFundingAdapter<S, __Return, __Parent>
    where
        __Parent: Clone + Send,
        __Parent: ::orga::client::AsyncCall<Call = <Accounts<S> as ::orga::call::Call>::Call>,
    {
        fn clone(&self) -> Self {
            let encoded_args = ::orga::encoding::Encode::encode(&self.args).unwrap();
            let cloned_args = ::orga::encoding::Decode::decode(encoded_args.as_slice()).unwrap();
            Self {
                parent: self.parent.clone(),
                args: cloned_args,
                _marker: std::marker::PhantomData,
            }
        }
    }
    impl<S: Symbol, __Parent, __Return> ::orga::client::AsyncCall
        for MethodGiveFromFundingAdapter<S, __Return, __Parent>
    where
        __Parent: Clone + Send,
        __Parent: ::orga::client::AsyncCall<Call = <Accounts<S> as ::orga::call::Call>::Call>,
        __Return: ::orga::call::Call,
        __Return::Call: Send + Sync,
    {
        type Call = <__Return as ::orga::call::Call>::Call;
        #[allow(
            clippy::let_unit_value,
            clippy::type_complexity,
            clippy::type_repetition_in_bounds,
            clippy::used_underscore_binding
        )]
        fn call<'life0, 'async_trait>(
            &'life0 mut self,
            call: Self::Call,
        ) -> ::core::pin::Pin<
            Box<
                dyn ::core::future::Future<Output = Result<()>>
                    + ::core::marker::Send
                    + 'async_trait,
            >,
        >
        where
            'life0: 'async_trait,
            Self: 'async_trait,
        {
            Box::pin(async move {
                if let ::core::option::Option::Some(__ret) =
                    ::core::option::Option::None::<Result<()>>
                {
                    return __ret;
                }
                let mut __self = self;
                let call = call;
                let __ret: Result<()> = {
                    let call_bytes = ::orga::encoding::Encode::encode(&call)?;
                    let parent_call =
                        <Accounts<S> as ::orga::call::Call>::Call::MethodGiveFromFunding(
                            __self.args.0,
                            call_bytes,
                        );
                    __self.parent.call(parent_call).await
                };
                #[allow(unreachable_code)]
                __ret
            })
        }
    }
    impl<S: Symbol, __Parent> Client<S, __Parent>
    where
        __Parent: Clone + Send,
        __Parent: ::orga::client::AsyncCall<Call = <Accounts<S> as ::orga::call::Call>::Call>,
    {
        pub fn give_from_funding(
            &mut self,
            amount: Amount,
        ) -> ::orga::client::CallChain<
            <Result<()> as ::orga::client::Client<
                MethodGiveFromFundingAdapter<S, Result<()>, __Parent>,
            >>::Client,
            MethodGiveFromFundingAdapter<S, Result<()>, __Parent>,
        > {
            let adapter = MethodGiveFromFundingAdapter {
                parent: self.parent.clone(),
                args: (amount,),
                _marker: std::marker::PhantomData,
            };
            let client = <Result<()> as ::orga::client::Client<
                MethodGiveFromFundingAdapter<S, _, __Parent>,
            >>::create_client(adapter.clone());
            ::orga::client::CallChain::new(client, adapter)
        }
    }
}
impl<S: Symbol, __Parent> ::orga::client::Client<__Parent> for Accounts<S>
where
    __Parent: Clone + Send,
{
    type Client = accounts_client::Client<S, __Parent>;
    fn create_client(parent: __Parent) -> Self::Client {
        accounts_client::Client::new(parent)
    }
}

// pub struct AccountsClient<S, U> {
//     parent: U,
//     _symbol: std::marker::PhantomData<S>,
// }

// impl<S: Symbol, U: Clone> Clone for AccountsClient<S, U> {
//     fn clone(&self) -> Self {
//         AccountsClient {
//             parent: self.parent.clone(),
//             _symbol: std::marker::PhantomData,
//         }
//     }
// }

// impl<S: Symbol, U: Clone> Client<U> for Accounts<S> {
//     type Client = AccountsClient<S, U>;

//     fn create_client(parent: U) -> Self::Client {
//         AccountsClient {
//             parent,
//             _symbol: std::marker::PhantomData,
//         }
//     }
// }

// type AccountsCall<S> = <Accounts<S> as Call>::Call;
// impl<S: Symbol, U: Clone + AsyncCall<Call = <Accounts<S> as Call>::Call>> AccountsClient<S, U> {
//     pub fn transfer(&mut self, to: Address, amount: Amount) -> CallChain<> {
//         let call = AccountsCall::<S>::MethodTransfer(to, amount, vec![]);
//         self.parent.call(call)
//     }
// }
