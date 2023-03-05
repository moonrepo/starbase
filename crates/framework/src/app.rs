use crate::context::Context;
use crate::system::{IntoSystemExecutor, System, SystemExecutor};

pub struct App {
    context: Context,
    initializers: Vec<SystemExecutor>,
}

impl App {
    pub fn new() -> Self {
        App {
            context: Context::default(),
            initializers: Vec::new(),
        }
    }

    pub fn add_initializer(&mut self, system: impl IntoSystemExecutor) -> &mut Self {
        self.initializers.push(system.into_system());
        self
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        let mut context = std::mem::take(&mut self.context);
        let initializers = self.initializers.drain(..).collect::<Vec<_>>();

        self.execute_systems(&mut context, initializers).await?;

        Ok(())
    }

    // Private

    async fn execute_systems(
        &mut self,
        context: &mut Context,
        systems: Vec<SystemExecutor>,
    ) -> anyhow::Result<()> {
        for system in systems {
            system.execute(context).await?;
        }

        Ok(())
    }
}
