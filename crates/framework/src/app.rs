use crate::context::Context;

pub struct App {
    context: Context,
}

impl App {
    pub fn new() -> Self {
        App {
            context: Context::new(),
        }
    }
}
