use crate::id_error::*;
use crate::id_regex::*;
use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use std::borrow::{Borrow, Cow};
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::ops::Deref;

/// A generic identifier.
#[derive(Clone, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(into = "String", try_from = "String")]
pub struct Id(CompactString);

impl Id {
    /// Create a new identifier with the provided string and validate
    /// its characters using a regex pattern.
    pub fn new<S: AsRef<str>>(id: S) -> Result<Self, IdError> {
        let id = id.as_ref();

        if !ID_PATTERN.is_match(id) {
            return Err(IdError(id.to_owned()));
        }

        Ok(Id::raw(id))
    }

    /// Clean the provided string to remove unwanted characters and
    /// return a valid identifier.
    pub fn clean<S: AsRef<str>>(id: S) -> Result<Self, IdError> {
        Self::new(
            ID_CLEAN_PATTERN
                .replace_all(id.as_ref(), "-")
                // Remove leading/trailing symbols
                .trim_matches(['@', '-', '_', '/', '.']),
        )
    }

    /// Create a new identifier with the provided string as-is.
    pub fn raw<S: AsRef<str>>(id: S) -> Id {
        Id(CompactString::new(id))
    }

    /// Convert the identifier into an environment variable name,
    /// by persisting alphanumeric characters and underscores,
    /// converting dashes to underscores, and removing everything else.
    pub fn into_env_var(self) -> String {
        self.to_env_var()
    }

    /// Convert the identifier into an [`OsString`].
    pub fn into_os_string(self) -> OsString {
        self.to_os_string()
    }

    /// Convert the identifier to an environment variable name,
    /// by persisting alphanumeric characters and underscores,
    /// converting dashes to underscores, and removing everything else.
    pub fn to_env_var(&self) -> String {
        let mut var = String::new();

        for ch in self.0.as_str().chars() {
            match ch {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '_' => {
                    var.push(ch);
                }
                '-' => {
                    var.push('_');
                }
                _ => {}
            }
        }

        var.to_uppercase()
    }

    /// Convert the identifier to an [`OsString`].
    pub fn to_os_string(&self) -> OsString {
        OsString::from(self.to_string())
    }

    /// Return the identifier as a [`CompactString`] reference.
    pub fn as_compact_str(&self) -> &CompactString {
        &self.0
    }

    /// Return the identifier as an [`OsStr`] reference.
    pub fn as_os_str(&self) -> &OsStr {
        OsStr::new(&self.0)
    }

    /// Return the identifier as a [`str`] reference.
    pub fn as_str(&self) -> &str {
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

impl AsRef<Id> for Id {
    fn as_ref(&self) -> &Id {
        self
    }
}

impl AsRef<str> for Id {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl AsRef<OsStr> for Id {
    fn as_ref(&self) -> &OsStr {
        OsStr::new(&self.0)
    }
}

impl Borrow<str> for Id {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl Borrow<OsStr> for Id {
    fn borrow(&self) -> &OsStr {
        OsStr::new(&self.0)
    }
}

macro_rules! gen_partial_eq {
    (os, $ty:ty) => {
        impl PartialEq<$ty> for Id {
            fn eq(&self, other: &$ty) -> bool {
                self.as_os_str() == other.as_os_str()
            }
        }
    };
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
gen_partial_eq!(os, OsString);
gen_partial_eq!(os, &OsString);

impl PartialEq<OsStr> for Id {
    fn eq(&self, other: &OsStr) -> bool {
        self.as_os_str() == other
    }
}

macro_rules! gen_try_from {
    (os, $ty:ty) => {
        impl TryFrom<$ty> for Id {
            type Error = IdError;

            fn try_from(value: $ty) -> Result<Self, Self::Error> {
                Self::new(value.to_string_lossy())
            }
        }
    };
    ($ty:ty) => {
        impl TryFrom<$ty> for Id {
            type Error = IdError;

            fn try_from(value: $ty) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }
    };
}

gen_try_from!(&str);
gen_try_from!(String);
gen_try_from!(&String);
gen_try_from!(Cow<'_, str>);
gen_try_from!(&Cow<'_, str>);
gen_try_from!(Box<str>);
gen_try_from!(&Box<str>);
gen_try_from!(os, &OsStr);
gen_try_from!(os, OsString);
gen_try_from!(os, &OsString);

impl From<Id> for String {
    fn from(value: Id) -> Self {
        value.to_string()
    }
}

#[cfg(feature = "schematic")]
impl schematic::Schematic for Id {
    fn schema_name() -> Option<String> {
        Some("Id".into())
    }

    fn build_schema(mut schema: schematic::SchemaBuilder) -> schematic::Schema {
        schema.string_default()
    }
}
