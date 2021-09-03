use std::any::Any;
use std::marker::PhantomData;

trait Ctx {
    type Value;
    fn resolve<T: 'static>(&self) -> Option<&T>;
}
pub struct Context<V, P> {
    value: Box<dyn Any>,
    parent: P,
    _v: PhantomData<V>,
}

impl<V, P: Ctx> Ctx for Context<V, P> {
    type Value = V;
    fn resolve<T: 'static>(&self) -> Option<&T> {
        let value = self.value.downcast_ref::<T>();
        match value {
            Some(value) => Some(value),
            None => self.parent.resolve::<T>(),
        }
    }
}

struct Root {}

impl Ctx for Root {
    type Value = Root;
    fn resolve<T: 'static>(&self) -> Option<&T> {
        None
    }
}

impl Context<Root, Root> {
    pub fn new() -> Context<Root, Root> {
        Context {
            value: Box::new(Root {}),
            parent: Root {},
            _v: PhantomData,
        }
    }
}

impl<V, P> Context<V, P> {
    pub fn wrap<T: 'static>(self, value: T) -> Context<T, Self> {
        let value: Box<dyn Any> = Box::new(value);
        Context {
            value,
            parent: self,
            _v: PhantomData,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_context() {
        let root_ctx = Context::new();
        let root_ctx = root_ctx
            .wrap(10 as i32)
            .wrap(())
            .wrap(20 as i64)
            .wrap(40 as i64)
            .wrap(30 as u32);

        let a: &i64 = root_ctx.resolve().unwrap();
        let b = root_ctx.resolve::<i32>().unwrap();
        let c = root_ctx.resolve::<u32>().unwrap();

        assert_eq!(*a, 40);
        assert_eq!(*b, 10);
        assert_eq!(*c, 30);
    }
}
