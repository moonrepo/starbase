use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use std::borrow::{Borrow, Cow};
use std::fmt;
use std::ops::Deref;

/// A generic identifier.
#[derive(Clone, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Id(CompactString);

impl Id {
    pub fn raw<S: AsRef<str>>(id: S) -> Id {
        Id(CompactString::new(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn as_compact_str(&self) -> &CompactString {
        &self.0
    }
}

impl fmt::Debug for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Deref for Id {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<str> for Id {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<Id> for Id {
    fn as_ref(&self) -> &Id {
        self
    }
}

impl Borrow<str> for Id {
    fn borrow(&self) -> &str {
        &self.0
    }
}

macro_rules! gen_partial_eq {
    ($ty:ty) => {
        impl PartialEq<$ty> for Id {
            fn eq(&self, other: &$ty) -> bool {
                self.0 == other
            }
        }
    };
}

gen_partial_eq!(str);
gen_partial_eq!(&str);
gen_partial_eq!(String);
gen_partial_eq!(&String);
gen_partial_eq!(Cow<'_, str>);
gen_partial_eq!(&Cow<'_, str>);
gen_partial_eq!(Box<str>);
gen_partial_eq!(&Box<str>);

macro_rules! gen_from {
    ($ty:ty) => {
        impl From<$ty> for Id {
            fn from(value: $ty) -> Self {
                Self::raw(value)
            }
        }
    };
}

gen_from!(&str);
gen_from!(String);
gen_from!(&String);
gen_from!(Cow<'_, str>);
gen_from!(&Cow<'_, str>);
gen_from!(Box<str>);
gen_from!(&Box<str>);
