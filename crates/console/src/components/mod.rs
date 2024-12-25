mod confirm;
mod entry;
mod input;
mod input_field;
mod layout;
mod list;
mod map;
mod notice;
mod progress;
mod section;
mod select;
mod styled_text;
mod table;

pub use confirm::*;
pub use entry::*;
pub use input::*;
pub use layout::*;
pub use list::*;
pub use map::*;
pub use notice::*;
pub use progress::*;
pub use section::*;
pub use select::*;
pub use styled_text::*;
pub use table::*;

// Re-export iocraft components
pub use iocraft::prelude::{Box as View, Button, Text};

use std::ops::Deref;
use std::sync::Arc;

pub struct Validator<'a, T>(Box<dyn Fn(T) -> Option<String> + Send + Sync + 'a>);

impl<T> Validator<'_, T> {
    /// Takes the handler, leaving a default-initialized handler in its place.
    pub fn take(&mut self) -> Self {
        std::mem::take(self)
    }
}

impl<T> Default for Validator<'_, T> {
    fn default() -> Self {
        Self(Box::new(|_| None))
    }
}

impl<'a, T, F> From<F> for Validator<'a, T>
where
    F: Fn(T) -> Option<String> + Send + Sync + 'a,
{
    fn from(f: F) -> Self {
        Self(Box::new(f))
    }
}

impl<'a, T: 'a> Deref for Validator<'a, T> {
    type Target = dyn Fn(T) -> Option<String> + Send + Sync + 'a;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone)]
pub enum OwnedOrShared<T: Clone> {
    Owned(T),
    Shared(Arc<T>),
}

impl<T: Clone> From<T> for OwnedOrShared<T> {
    fn from(value: T) -> OwnedOrShared<T> {
        Self::Owned(value)
    }
}

impl<T: Clone> From<Arc<T>> for OwnedOrShared<T> {
    fn from(value: Arc<T>) -> OwnedOrShared<T> {
        Self::Shared(value)
    }
}

impl<T: Clone> Deref for OwnedOrShared<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Owned(inner) => inner,
            Self::Shared(inner) => inner,
        }
    }
}
