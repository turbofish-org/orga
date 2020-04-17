use orga::{Encode, Decode};

#[derive(Encode, Decode, PartialEq, Debug)]
struct Foo {
    a: u8,
    b: Option<u8>
}

#[test]
fn encode_decode() {
    let value = Foo { a: 5, b: Some(6) };
    let bytes = value.encode().unwrap();
    assert_eq!(bytes.as_slice(), &[5, 1, 6]);
    let decoded_value = Foo::decode(bytes.as_slice()).unwrap();
    assert_eq!(decoded_value, value);
}
