use super::Read;

/// A key/value pair emitted from an [`Iter`](trait.Iter.html).
pub type Entry<'a> = (&'a [u8], &'a [u8]);

/// An interface for `Store` implementations which can create iterators over
/// their key/value pairs.
pub trait Iter {
    type Iter<'a>: Iterator<Item = Entry<'a>>;

    // TODO: use ranges rather than just start key
    fn iter_from(&self, start: &[u8]) -> Self::Iter<'_>;

    fn iter(&self) -> Self::Iter<'_> {
        self.iter_from(&[])
    }
}
