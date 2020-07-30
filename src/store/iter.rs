use std::ops::Deref;

pub type Entry<'a> = (&'a [u8], &'a [u8]);

pub trait Iter<'a, 'b: 'a> {
    type Iter: Iterator<Item = Entry<'b>>;

    // TODO: use ranges rather than just start key
    fn iter_from(&'a self, start: &[u8]) -> Self::Iter;

    fn iter(&'a self) -> Self::Iter {
        self.iter_from(&[])
    }
}

impl<'a, 'b: 'a, T, I> Iter<'a, 'b> for T
where
    I: Iter<'a, 'b> + 'a,
    T: Deref<Target = I>,
{
    type Iter = I::Iter;

    fn iter_from(&'a self, start: &[u8]) -> I::Iter {
        (**self).iter_from(start)
    }
}
