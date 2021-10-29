pub trait Balance<T> {
    fn balance(&self) -> T;
}
