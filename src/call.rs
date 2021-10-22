#[cfg(test)]
use mutagen::mutate;

use crate::encoding::{Decode, Encode};
use crate::{Error, Result};
use std::cell::RefCell;
use std::rc::Rc;

pub use orga_macros::{call, Call};

pub trait Call {
    type Call: Encode + Decode;

    fn call(&mut self, call: Self::Call) -> Result<()>;
}

impl<T: Call> Call for &mut T {
    type Call = T::Call;

    fn call(&mut self, call: Self::Call) -> Result<()> {
        (*self).call(call)
    }
}

impl<T: Call> Call for Rc<RefCell<T>> {
    type Call = T::Call;

    fn call(&mut self, call: Self::Call) -> Result<()> {
        self.borrow_mut().call(call)
    }
}

impl<T: Call> Call for RefCell<T> {
    type Call = T::Call;

    fn call(&mut self, call: Self::Call) -> Result<()> {
        self.borrow_mut().call(call)
    }
}

impl<T: Call> Call for Result<T> {
    type Call = T::Call;

    fn call(&mut self, call: Self::Call) -> Result<()> {
        match self {
            Ok(inner) => inner.call(call),
            Err(err) => Err(Error::Call(err.to_string())),
        }
    }
}

impl<T: Call> Call for Option<T> {
    type Call = T::Call;

    fn call(&mut self, call: Self::Call) -> Result<()> {
        match self {
            Some(inner) => inner.call(call),
            None => Err(Error::Call("Call option is None".into())),
        }
    }
}

macro_rules! noop_impl {
    ($type:ty) => {
        impl Call for $type {
            type Call = ();

            fn call(&mut self, _: ()) -> Result<()> {
                Ok(())
            }
        }
    };
}

noop_impl!(());
noop_impl!(bool);
noop_impl!(u8);
noop_impl!(u16);
noop_impl!(u32);
noop_impl!(u64);
noop_impl!(u128);
noop_impl!(i8);
noop_impl!(i16);
noop_impl!(i32);
noop_impl!(i64);
noop_impl!(i128);

impl<T: Call> Call for (T,) {
    type Call = T::Call;

    fn call(&mut self, call: Self::Call) -> Result<()> {
        self.0.call(call)
    }
}

#[derive(Encode, Decode)]
pub enum Tuple2Call<T, U>
where
    T: Call,
    U: Call,
{
    Field0(T::Call),
    Field1(U::Call),
}

impl<T, U> Call for (T, U)
where
    T: Call,
    U: Call,
{
    type Call = Tuple2Call<T, U>;

    fn call(&mut self, call: Self::Call) -> Result<()> {
        match call {
            Tuple2Call::Field0(call) => self.0.call(call),
            Tuple2Call::Field1(call) => self.1.call(call),
        }
    }
}

#[derive(Encode, Decode)]
pub enum Tuple3Call<T, U, V>
where
    T: Call,
    U: Call,
    V: Call,
{
    Field0(T::Call),
    Field1(U::Call),
    Field2(V::Call),
}

impl<T, U, V> Call for (T, U, V)
where
    T: Call,
    U: Call,
    V: Call,
{
    type Call = Tuple3Call<T, U, V>;

    fn call(&mut self, call: Self::Call) -> Result<()> {
        match call {
            Tuple3Call::Field0(call) => self.0.call(call),
            Tuple3Call::Field1(call) => self.1.call(call),
            Tuple3Call::Field2(call) => self.2.call(call),
        }
    }
}

#[derive(Encode, Decode)]
pub enum Tuple4Call<T, U, V, W>
where
    T: Call,
    U: Call,
    V: Call,
    W: Call,
{
    Field0(T::Call),
    Field1(U::Call),
    Field2(V::Call),
    Field3(W::Call),
}

impl<T, U, V, W> Call for (T, U, V, W)
where
    T: Call,
    U: Call,
    V: Call,
    W: Call,
{
    type Call = Tuple4Call<T, U, V, W>;

    fn call(&mut self, call: Self::Call) -> Result<()> {
        match call {
            Tuple4Call::Field0(call) => self.0.call(call),
            Tuple4Call::Field1(call) => self.1.call(call),
            Tuple4Call::Field2(call) => self.2.call(call),
            Tuple4Call::Field3(call) => self.3.call(call),
        }
    }
}

impl<T: Call, const N: usize> Call for [T; N] {
    type Call = (u64, T::Call);

    fn call(&mut self, call: Self::Call) -> Result<()> {
        let (index, subcall) = call;
        let index = index as usize;

        if index >= N {
            return Err(Error::Call("Call index out of bounds".into()));
        }

        self[index].call(subcall)
    }
}

#[cfg_attr(test, mutate)]
pub fn maybe_call<T>(value: T, subcall: Vec<u8>) -> Result<()> {
    MaybeCallWrapper(value).maybe_call(subcall)
}

trait MaybeCall {
    fn maybe_call(&mut self, call_bytes: Vec<u8>) -> Result<()>;
}

impl<T> MaybeCall for T {
    default fn maybe_call(&mut self, _call_bytes: Vec<u8>) -> Result<()> {
        Err(Error::Call("Call is not implemented".into()))
    }
}

struct MaybeCallWrapper<T>(T);

impl<T: Call> MaybeCall for MaybeCallWrapper<T> {
    fn maybe_call(&mut self, call_bytes: Vec<u8>) -> Result<()> {
        let call = Decode::decode(call_bytes.as_slice())?;
        self.0.call(call)
    }
}
