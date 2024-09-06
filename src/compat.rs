use std::cell::RefCell;

thread_local! {
    static COMPAT_MODE: RefCell<bool> = const { RefCell::new(false) };
}

/// Check if executing in compatibility mode.
pub fn compat_mode() -> bool {
    COMPAT_MODE.with(|compat_mode| *compat_mode.borrow())
}

/// Set compatibility mode.
pub fn set_compat_mode(compat_mode: bool) {
    COMPAT_MODE.set(compat_mode);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        encoding::{Decode, Encode},
        orga,
        state::State,
        store::Store,
        Result,
    };

    use serial_test::serial;

    #[orga]
    #[derive(Clone)]
    struct Foo {
        bar: u8,
    }

    #[test]
    #[serial]
    fn compat_mode_simple_type() -> Result<()> {
        let foo = Foo { bar: 123 };
        let store = Store::default();

        set_compat_mode(true);
        assert_eq!(foo.encode()?, vec![123]);
        assert_eq!(Foo::decode(vec![123].as_slice()).unwrap().bar, 123);
        assert_eq!(
            Foo::load(store.clone(), &mut vec![123].as_slice())?.bar,
            123
        );
        let mut bytes = vec![];
        foo.clone().flush(&mut bytes)?;
        assert_eq!(bytes, vec![123]);

        set_compat_mode(false);
        assert_eq!(foo.encode()?, vec![0, 123]);
        assert_eq!(Foo::decode(vec![0, 123].as_slice()).unwrap().bar, 123);
        assert_eq!(Foo::load(store, &mut vec![0, 123].as_slice())?.bar, 123);
        let mut bytes = vec![];
        foo.flush(&mut bytes)?;
        assert_eq!(bytes, vec![0, 123]);

        Ok(())
    }
}
