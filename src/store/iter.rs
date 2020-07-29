use std::ops::Deref;

use super::Read;

pub type Entry<'a> = (&'a [u8], &'a [u8]);

pub trait Iter<'a, 'b: 'a> {
    type Iter: Iterator<Item = Entry<'b>>;

    fn iter(&'a self, start: &[u8]) -> Self::Iter;
}

impl<'a, 'b: 'a, T, I> Iter<'a, 'b> for T
where
    I: Iter<'a, 'b> + 'a,
    T: Deref<Target = I>,
{
    type Iter = I::Iter;

    fn iter(&'a self, start: &[u8]) -> I::Iter {
        (**self).iter(start)
    }
}
