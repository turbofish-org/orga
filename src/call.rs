//! State mutations triggerable by network messages.

use crate::encoding::{Decode, Encode};
use crate::{Error, Result};
use std::cell::RefCell;
use std::error::Error as StdError;
use std::io::Read;
use std::rc::Rc;
use std::result::Result as StdResult;

pub use orga_macros::{build_call, call_block, FieldCall};

/// The prefix offset for method calls, added to avoid conflicts with state or
/// method query prefixes for fields.
pub const PREFIX_OFFSET: u8 = 0x40;

/// A trait for types to describe and expose state mutations publicly (e.g.
/// transactions).
///
/// Calls are typically composed hierarchically from
/// [FieldCall] and [MethodCall]. The [FieldCall] derive macro
/// generates variants for each field tagged `#[call]`, and [call_block]
/// generates a [MethodCall] enum with variants for each public method tagged
/// `#[call]` in an impl block for that type.
///
/// `Call` may also be implemented manually to enable more complex behavior,
/// such as in [crate::plugins::SignerPlugin] or [crate::plugins::PayablePlugin]
pub trait Call {
    /// The message type for the call, which must implement [Encode] and
    /// [Decode]
    type Call: Encode + Decode + std::fmt::Debug + Send + Sync;

    /// Perform the call.
    fn call(&mut self, call: Self::Call) -> Result<()>;
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

impl<T: Call, E: StdError> Call for StdResult<T, E> {
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

impl<T: Call> Call for Vec<T> {
    type Call = (u32, T::Call);

    fn call(&mut self, call: Self::Call) -> Result<()> {
        let (index, subcall) = call;
        self.get_mut(index as usize)
            .ok_or_else(|| Error::App("Index out of bounds".to_string()))?
            .call(subcall)
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

/// Call for 2-field tuples.
#[derive(Debug, Encode, Decode)]
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
    T: Call + std::fmt::Debug,
    U: Call + std::fmt::Debug,
{
    type Call = Tuple2Call<T, U>;

    fn call(&mut self, call: Self::Call) -> Result<()> {
        match call {
            Tuple2Call::Field0(call) => self.0.call(call),
            Tuple2Call::Field1(call) => self.1.call(call),
        }
    }
}

/// Call for 3-field tuples.
#[derive(Debug, Encode, Decode)]
pub enum Tuple3Call<T, U, V>
where
    T: Call + std::fmt::Debug,
    U: Call + std::fmt::Debug,
    V: Call + std::fmt::Debug,
{
    Field0(T::Call),
    Field1(U::Call),
    Field2(V::Call),
}

impl<T, U, V> Call for (T, U, V)
where
    T: Call + std::fmt::Debug,
    U: Call + std::fmt::Debug,
    V: Call + std::fmt::Debug,
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

/// Call for 4-field tuples.
#[derive(Debug, Encode, Decode)]
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
    T: Call + std::fmt::Debug,
    U: Call + std::fmt::Debug,
    V: Call + std::fmt::Debug,
    W: Call + std::fmt::Debug,
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

/// Represents either a field or method call item.
///
/// The encoding of this type handles the prefix byte convention for fields vs.
/// methods.
pub enum Item<T: std::fmt::Debug, U: std::fmt::Debug> {
    Field(T),
    Method(U),
}

impl<T: std::fmt::Debug, U: std::fmt::Debug> std::fmt::Debug for Item<T, U> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Item::Field(field) => field.fmt(f),
            Item::Method(method) => method.fmt(f),
        }
    }
}

impl<T: Encode + std::fmt::Debug, U: Encode + std::fmt::Debug> Encode for Item<T, U> {
    fn encode_into<W: std::io::Write>(&self, dest: &mut W) -> ed::Result<()> {
        match self {
            Item::Field(field) => {
                field.encode_into(dest)?;
            }
            Item::Method(method) => {
                let mut bytes = method.encode()?;
                if !bytes.is_empty() && bytes[0] < PREFIX_OFFSET {
                    bytes[0] += PREFIX_OFFSET;
                } else {
                    return Err(ed::Error::UnencodableVariant);
                }
                dest.write_all(&bytes)?;
            }
        }

        Ok(())
    }

    fn encoding_length(&self) -> ed::Result<usize> {
        match self {
            Item::Field(field) => field.encoding_length(),
            Item::Method(method) => method.encoding_length(),
        }
    }
}

impl<T: Decode + std::fmt::Debug, U: Decode + std::fmt::Debug> Decode for Item<T, U> {
    fn decode<R: std::io::Read>(input: R) -> ed::Result<Self> {
        let mut input = input;
        let mut buf = [0u8; 1];
        input.read_exact(&mut buf)?;
        let prefix = buf[0];

        if prefix < PREFIX_OFFSET {
            let input = buf.chain(input);
            let field = T::decode(input)?;
            Ok(Item::Field(field))
        } else {
            let bytes = [prefix - PREFIX_OFFSET; 1];
            let input = bytes.chain(input);
            let method = U::decode(input)?;
            Ok(Item::Method(method))
        }
    }
}

/// A trait for complex types whose fields may also be [Call].
///
/// The [FieldCall] derive macro generates variants for each field tagged
/// `#[call]`, and the unwrapped call is passed along to that field's [Call]
/// implementation.
///
/// The encoding of the `FieldCall` uses the field's [State] prefix by default.
pub trait FieldCall {
    /// The encodable message type for calls to this type's fields.
    type FieldCall: Encode + Decode + std::fmt::Debug + Send + Sync = ();

    /// Perform the field call.
    fn field_call(&mut self, call: Self::FieldCall) -> Result<()>;
}

/// A trait for types that expose public methods as calls (via the `#[call]`
/// attribute and `#[call_block]` macro).
///
/// Method calls are assigned an incrementing byte prefix, starting from
/// [PREFIX_OFFSET] to avoid conflicts with fields or method queries.
///
/// After the prefix byte, the remaining bytes of the encoded method call are
/// the encoded method arguments (which must each implement [Encode] and
/// [Decode]).
pub trait MethodCall {
    /// The encodable message type for calls to this type's methods.
    type MethodCall: Encode + Decode + std::fmt::Debug + Send + Sync = ();

    /// Perform the method call.
    fn method_call(&mut self, call: Self::MethodCall) -> Result<()>;
}

impl<T> Call for T
where
    T: FieldCall + MethodCall,
{
    type Call = Item<T::FieldCall, T::MethodCall>;

    fn call(&mut self, call: Self::Call) -> Result<()> {
        match call {
            Item::Field(call) => self.field_call(call),
            Item::Method(call) => self.method_call(call),
        }
    }
}

impl<T> MethodCall for T {
    default type MethodCall = ();
    default fn method_call(&mut self, _call: Self::MethodCall) -> Result<()> {
        Err(Error::Call("Method not found".to_string()))
    }
}

/// A trait for building calls statically with the [build_call] macro.
pub trait BuildCall<const ID: &'static str>: Call + Sized {
    /// The type for this type's field named `ID`
    type Child: Call;
    /// The arguments to the call.
    type Args = ();
    /// Build the call.
    fn build_call<F: Fn(CallBuilder<Self::Child>) -> <Self::Child as Call>::Call>(
        f: F,
        args: Self::Args,
    ) -> Self::Call;
}

/// Helper for building calls, used in the [build_call] macro.
pub struct CallBuilder<T> {
    _phantom: std::marker::PhantomData<fn(T)>,
}

impl<T> CallBuilder<T> {
    pub fn new() -> Self {
        Self {
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<T> Default for CallBuilder<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> CallBuilder<T> {
    /// Builds a call for this type, using the provided function to build the
    /// child's call.
    pub fn build_call<
        const ID: &'static str,
        F: Fn(CallBuilder<<T as BuildCall<ID>>::Child>) -> <<T as BuildCall<ID>>::Child as Call>::Call,
    >(
        &self,
        f: F,
        args: <T as BuildCall<ID>>::Args,
    ) -> T::Call
    where
        T: BuildCall<ID>,
    {
        T::build_call(f, args)
    }
}

impl<T> CallBuilder<T> {
    /// Makes a CallBuilder for the provided type.
    pub fn make<U: std::ops::Deref<Target = T>>(_value: U) -> Self {
        Self::new()
    }
}
