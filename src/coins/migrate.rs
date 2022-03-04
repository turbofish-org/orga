use v1::state::State as OldState;

#[derive(Debug, Clone)]
pub struct Sym(());

impl OldState for Sym {
    type Encoding = ();

    fn create(_store: v1::store::Store, _data: Self::Encoding) -> v1::Result<Self> {
        unimplemented!()
    }

    fn flush(self) -> v1::Result<Self::Encoding> {
        unimplemented!()
    }
}

impl From<Sym> for () {
    fn from(_: Sym) -> Self {}
}

impl v1::coins::Symbol for Sym {}
