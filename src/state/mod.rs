use crate::store::*;
use crate::Result;
pub use orga_macros::State;
use std::cell::RefCell;

/// A trait for types which provide a higher-level API for data stored within a
/// [`store::Store`](../store/trait.Store.html).
///
/// These types can be complex types like collections (e.g. maps), or simple
/// data types (e.g. account structs).
pub trait State<S = DefaultBackingStore>: Sized {
    /// A type which provides the binary encoding of data stored in the type's
    /// root key/value entry. When being written to a store, `State` values will
    /// be converted to this type and then encoded into bytes. When being
    /// loaded, bytes will be decoded as this type and then passed to `create()`
    /// along with access to the store to construct the `State` value.
    ///
    /// For complex types which store all of their data in child key/value
    /// entries, this will often be the unit tuple (`()`) in order to provide a
    /// no-op encoding.
    ///
    /// For simple data types, this will often be `Self` since a separate type
    /// is not needed.
    type Encoding: ed::Encode + ed::Decode + From<Self>;

    /// Creates an instance of the type from a dedicated substore (`store`) and
    /// associated data (`data`).
    ///
    /// Implementations which only represent simple data and do not need access
    /// to the store can just ignore the `store` argument.
    ///
    /// This method will be called by some external container and will rarely be
    /// explicitly called to construct an instance of the type.
    fn create(store: Store<S>, data: Self::Encoding) -> Result<Self>
    where
        S: Read;

    /// Called when the data is to be written to the backing store, and converts
    /// the instance into `Self::Encoding` in order to specify how it should be
    /// represented in binary bytes.
    ///
    /// Note that the type does not write its own binary representation, it is
    /// assumed some external container will store the bytes in a relevant part
    /// of the store. The type does however need to write any child key/value
    /// entries (often by calling child `State` types' `flush()` implementations
    /// then storing their resulting binary representations) to the store if
    /// necessary.
    fn flush(self) -> Result<Self::Encoding>
    where
        S: Write;
}

impl<T: ed::Encode + ed::Decode, S> State<S> for T {
    type Encoding = Self;

    #[inline]
    fn create(_: Store<S>, value: Self) -> Result<Self> {
        Ok(value)
    }

    #[inline]
    fn flush(self) -> Result<Self::Encoding> {
        Ok(self)
    }
}

impl<T: State<S>, S> State<S> for RefCell<T> {
    type Encoding = T::Encoding;

    fn create(store: Store<S>, data: Self::Encoding) -> Result<Self> {
        Ok(RefCell::new(T::create(store, data)?))
    }
}
