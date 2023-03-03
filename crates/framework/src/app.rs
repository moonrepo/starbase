use crate::context::Context;

#[derive(Debug)]
pub struct App {
    context: Context,
}

impl App {
    pub fn new() -> Self {
        App {
            context: Context::new(),
        }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        dbg!(&self);

        Ok(())
    }
}
