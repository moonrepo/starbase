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
pub use styled_text::*;
pub use table::*;

// Re-export iocraft components
pub use iocraft::prelude::{Box as View, Button, Text};

pub struct Validator<'a, T>(Box<dyn Fn(T) -> Option<String> + Send + Sync + 'a>);

impl<'a, T> Validator<'a, T> {
    /// Takes the handler, leaving a default-initialized handler in its place.
    pub fn take(&mut self) -> Self {
        std::mem::take(self)
    }
}

impl<'a, T> Default for Validator<'a, T> {
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

impl<'a, T: 'a> std::ops::Deref for Validator<'a, T> {
    type Target = dyn Fn(T) -> Option<String> + Send + Sync + 'a;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
