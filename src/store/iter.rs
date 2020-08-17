use std::ops::Deref;
use super::Read;

/// A key/value pair emitted from an [`Iter`](trait.Iter.html).
pub type Entry<'a> = (&'a [u8], &'a [u8]);

/// An interface for `Store` implementations which can create iterators over
/// their key/value pairs.
pub trait Iter<'a, 'b: 'a>: Read {
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
