// This module is adapted from basecoin-rs

use crate::Result;
use ibc::core::ics24_host::validate::validate_identifier;
use std::convert::{TryFrom, TryInto};
use std::fmt::{Debug, Display, Formatter};
use std::ops::Deref;
use std::str::from_utf8;

/// A newtype representing a valid ICS024 identifier.
/// Implements `Deref<Target=String>`.
#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Clone)]
pub struct Identifier(String);

impl Identifier {
    /// Identifiers MUST be non-empty (of positive integer length).
    /// Identifiers MUST consist of characters in one of the following categories only:
    /// * Alphanumeric
    /// * `.`, `_`, `+`, `-`, `#`
    /// * `[`, `]`, `<`, `>`
    fn validate(s: impl AsRef<str>) -> Result<()> {
        let s = s.as_ref();

        // give a `min` parameter of 0 here to allow id's of arbitrary
        // length as inputs; `validate_identifier` itself checks for
        // empty inputs and returns an error as appropriate
        validate_identifier(s, 0, s.len()).map_err(|e| crate::Error::Ibc(e.to_string()))
    }
}

impl Deref for Identifier {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TryFrom<String> for Identifier {
    type Error = crate::Error;

    fn try_from(s: String) -> Result<Self> {
        Identifier::validate(&s).map(|_| Self(s))
    }
}

impl Display for Identifier {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A newtype representing a valid ICS024 `Path`.
#[derive(Debug, Ord, PartialOrd, Eq, PartialEq, Clone)]

pub struct Path(Vec<Identifier>);

impl TryFrom<String> for Path {
    type Error = crate::Error;

    fn try_from(s: String) -> Result<Self> {
        let mut identifiers = vec![];
        let parts = s.split('/'); // split will never return an empty iterator
        for part in parts {
            identifiers.push(Identifier::try_from(part.to_owned())?);
        }
        Ok(Self(identifiers))
    }
}

impl TryFrom<&[u8]> for Path {
    type Error = crate::Error;

    fn try_from(value: &[u8]) -> Result<Self> {
        let s = from_utf8(value).map_err(|_| crate::Error::Ibc("Malformed path".to_string()))?;
        s.to_owned().try_into()
    }
}

impl From<Identifier> for Path {
    fn from(id: Identifier) -> Self {
        Self(vec![id])
    }
}

impl Display for Path {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            self.0
                .iter()
                .map(|iden| iden.as_str().to_owned())
                .collect::<Vec<String>>()
                .join("/")
        )
    }
}
