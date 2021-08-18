use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use syn::*;
use quote::quote;

pub fn attr(args: TokenStream, input: TokenStream) -> TokenStream {
  let args = parse_macro_input!(args as AttributeArgs);
  let input = parse_macro_input!(input as ItemFn);



  input.into()
}

mod x {
  mod orga {
    type Result<T> = std::result::Result<T, &'static str>;

    pub trait FieldQuery {
      type Query: Encode + Decode;
      type Res: Encode + Decode;
    }
    
    impl<T> FieldQuery for T {
      default type Query = ();
      default type Res = ();
    }

    pub trait MethodQuery {
      type Query: Encode + Decode;
      type Res: Encode + Decode;
    }

    impl<T> MethodQuery for T {
      default type Query = ();
      default type Res = ();
    }

    pub trait Query: FieldQuery + MethodQuery {
      fn query(&self, query: QueryCall<Self>) -> QueryRes<Self>;
    }

    #[derive(Debug, Encode, Decode)]
    pub enum QueryCall<T: Query> {
      Field(<T as FieldQuery>::Query),
      Method(<T as MethodQuery>::Query),
    }

    #[derive(Debug, Encode, Decode)]
    pub enum QueryRes<T: Query> {
      Field(<T as FieldQuery>::Res),
      Method(<T as MethodQuery>::Res),
    }

    pub trait Call {
      type Op: Encode + Decode;
      // TODO: type Res: Encode + Decode;

      fn step(&mut self, op: Self::Op) -> Result<()>;
    }

    pub trait Client<T: Query + Call> {
      fn query<F, R>(&self, query: T::Query, check: F) -> Result<R>
        where F: Fn(T::Res) -> Result<R>;
      
      fn step(&mut self, op: T::Op) -> Result<()>;
    }

    pub trait MakeClient<T: Client> {
      type Client;

      fn make_client(client: T) -> Self::Client;
    }

    pub struct TendermintClient<T> {
      marker: std::marker::PhantomData<T>,
    }

    impl<T> TendermintClient<T> {
      // TODO: take tendermint node url as argument
      pub fn new() -> Self {
        TendermintClient {
          marker: std::marker::PhantomData,
        }
      }
    }

    impl<T: Query + Call> Client<T> for TendermintClient<T> {
      fn query<F, R>(&self, query: T::Query, check: F) -> Result<R>
        where F: Fn(T::Res) -> Result<R>
      {
        todo!()
      }

      fn step(&mut self, op: T::Op) -> Result<()> {
        todo!()
      }
    }
  }

  pub struct Counter {
    count: u32,
    map: HashMap<u32, u32>,
  }

  impl Counter {
    pub fn increment(&mut self, n: u32) -> Result<()> {
      if n != self.count {
        return Err("Incorrect count");
      }

      self.count += 1;
    }

    pub fn count(&self) -> u32 {
      self.count
    }
  }

  // first expansion

  pub struct Client<T> {
    client: T,
  }
  
  impl<T> MakeClient<T> for Counter {
    type Client = Client<T>;

    fn make_client(client: T) -> Self::Client {
      Client { client }
    }
  }

  #[derive(Debug, Encode, Decode)]
  pub enum FieldQuery {
    #[named_encoding]
    Count(::orga::QueryCall<u32>),
    Map(::orga::QueryCall<HashMap<u32, u32>>),
  }

  #[derive(Debug, Encode, Decode)]
  pub enum FieldQueryRes {
    Count(::orga::QueryRes<u32>),
    Map(::orga::QueryRes<HashMap<u32, u32>>),
  }

  // second expansion

  #[named_encoding]
  #[derive(Debug, Encode, Decode)]
  pub enum MethodQuery {
    Count,
  }

  impl Encode for MethodQuery {
    fn encode_into<W: Write>(&self, dest: &mut W) -> Result<()> {
      match self {
        MethodQuery::Count => {
          let name = "Count";
          dest.write_all(&[name.len() as u8])?;
          dest.write_all(name.as_bytes())?;
          Ok(())
        }
      }
    }

    fn encoding_length(&self) -> Result<usize> {
      MethodQuery::Count => {
        let name = "Count";
        Ok(name.len() + 1)
      }
    }
  }

  impl Decode for MethodQuery {
    fn decode<R: Read>(input: R) -> Result<Self> {
      let name_len = [0; 1];
      input.read_exact(&mut name_len[..])?;
      let name_len = name_len[0];

      // TODO: fail early if name is an unexpected length
      let name = [0; name_len];
      input.read_exact(&mut name_len[..])?;
      let name = name_len.as_string();
      
      match name {
        b"count" => Ok(MethodQuery::Count),
        _ => Err("Unknown query"),
      }
    }

    // TODO: decode_into
  }
  
  #[derive(Debug, Encode, Decode)]
  pub enum MethodQueryRes {
    Count(u32),
  }

  impl Encode for MethodQueryRes {
    fn encode_into<W: Write>(&self, dest: &mut W) -> Result<()> {
      match self {
        MethodQuery::Count(value) => {
          let name = "count";
          dest.write_all(&[name.len() as u8])?;
          dest.write_all(name.as_bytes())?
          value.encode_into(&mut dest)?;
          Ok(())
        }
      }
    }

    fn encoding_length(&self) -> Result<usize> {
      MethodQuery::Count(value) => {
        let name = "count";
        Ok(name.len() + 1 + value.encoding_length())
      }
    }
  }

  impl Decode for MethodQueryRes {
    fn decode<R: Read>(input: R) -> Result<Self> {
      let name_len = [0; 1];
      input.read_exact(&mut name_len[..])?;
      let name_len = name_len[0];

      // TODO: fail early if name is an unexpected length
      let name = [0; name_len];
      input.read_exact(&mut name_len[..])?;
      let name = name_len.as_string();
      
      match name {
        b"count" => {
          let value = Decode::decode(input)?;
          Ok(MethodQueryRes::Count(value))
        }
        _ => Err("Unknown query response"),
      }
    }

    // TODO: decode_into
  }

  impl orga::MethodQuery for Counter {
    type Query = MethodQuery;
    type Res = MethodQueryRes;

    fn query(&self, query: MethodQuery) -> Result<MethodQueryRes> {
      match query {
        Count => Ok(MethodQueryRes::Count(self.count())),
      }
    }
  }

  pub enum Call {
    Increment(u32),
  }

  impl Encode for Call {
    fn encode_into<W: Write>(&self, dest: &mut W) -> Result<()> {
      match self {
        MethodQuery::Increment(count) => {
          let name = "increment";
          dest.write_all(&[name.len() as u8])?;
          dest.write_all(name.as_bytes())?
          count.encode_into(&mut dest)?;
          Ok(())
        }
      }
    }

    fn encoding_length(&self) -> Result<usize> {
      MethodQuery::Increment(count) => {
        let name = "increment";
        Ok(name.len() + 1 + count.encoding_length())
      }
    }
  }

  impl Decode for Call {
    fn decode<R: Read>(input: R) -> Result<Self> {
      let name_len = [0; 1];
      input.read_exact(&mut name_len[..])?;
      let name_len = name_len[0];

      // TODO: fail early if name is an unexpected length
      let name = [0; name_len];
      input.read_exact(&mut name_len[..])?;
      let name = name_len.as_string();
      
      match name {
        b"increment" => {
          let count = Decode::decode(input)?;
          Ok(Call::Increment(count))
        }
        _ => Err("Unknown op"),
      }
    }

    // TODO: decode_into
  }

  impl orga::Call for Counter {
    type Op = Call;

    fn step(&mut self, op: Call) -> Result<()> {
      match op {
        Increment(n) => self.increment(n)?,
      };

      Ok(())
    }
  }

  impl Client {
    fn increment(&mut self, n: u32) -> Result<()> {
      self.client.step(Call::Increment(n))
    }

    fn count(&self) -> Result<u32> {
      self.client.query(Counter::Count, |res| match res {
        Count(a) => Ok(a),
        _ => Err(()),
      })
    }
  }
}
