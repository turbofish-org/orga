#![feature(stmt_expr_attributes)]
use orga::channels;

#[channels(Bar, Baz)]
#[derive(Clone)]
pub struct Foo<T> {
    pub a: u32,

    #[channel(Bar)]
    pub b: u16,

    pub c: T,
}

#[channels(Bar, Baz)]
impl<T: Default> Foo<T> {
    #[channel(Bar)]
    const N: u32 = 3;

    #[channel(Baz)]
    const N: u32 = 1;

    pub fn new() -> Self {
        Self {
            #[channel(Bar)]
            a: 1,
            #[channel(Baz)]
            a: 2,
            #[channel(Bar)]
            b: 3,
            c: T::default(),
        }
    }

    pub fn my_method(&self, #[channel(Bar)] n: u32) -> u32 {
        let mut x = 3;
        #[channel(Baz)]
        {
            x += 1;
            x *= 2;
        }

        (#[channel(Bar, Baz)]
        x) += 1;

        #[channel(Bar)]
        let res = self.a + x + n;

        #[channel(Baz)]
        let res = self.a + x;

        let res = self.add_some(res);

        self.add_more(res)
    }

    #[channel(Bar)]
    fn add_some(&self, n: u32) -> u32 {
        n + 3
    }

    #[channel(Baz)]
    fn add_some(&self, n: u32) -> u32 {
        n + 2
    }

    fn add_more(&self, n: u32) -> u32 {
        n + Self::N
    }
}

#[channels(Bar, Baz)]
impl<T: Default> Default for Foo<T>
where
    T: Clone,
{
    fn default() -> Self {
        Self {
            #[channel(Bar)]
            a: 1,
            #[channel(Baz)]
            a: 2,
            #[channel(Bar)]
            b: 3,
            c: T::default(),
        }
    }
}

#[test]
fn two_channels() {
    let a = FooBar::<u8>::new();
    let b = FooBaz::<u8>::new();
    assert_eq!(a.a, 1);
    assert_eq!(b.a, 2);
    assert_eq!(a.my_method(3), b.my_method());
}
