pub use orga_macros::Child;
pub trait Child {
    type Child;
}

pub struct Field<T, const ID: u128>(std::marker::PhantomData<T>);
