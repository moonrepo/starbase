use core::future::Future;
use futures::future::BoxFuture;

pub type EventResult<R> = anyhow::Result<EventState<R>>;
pub type EventFutureResult<R> = BoxFuture<'static, EventResult<R>>;

pub trait Event: Send + Sync {
    type ReturnValue;
}

pub enum EventState<R> {
    Continue,
    Stop,
    Return(R),
}

pub trait ListenerFunc<E: Event>: Send + Sync {
    fn call(&self, event: &mut E) -> EventFutureResult<E::ReturnValue>;
}

impl<T: Send + Sync, F, E: Event> ListenerFunc<E> for T
where
    T: Fn(&mut E) -> F,
    F: Future<Output = EventResult<E::ReturnValue>> + Send + 'static,
{
    fn call(&self, event: &mut E) -> EventFutureResult<E::ReturnValue> {
        Box::pin(self(event))
    }
}

pub struct Listener<E: Event> {
    callback: Option<Box<dyn ListenerFunc<E>>>,
    once: bool,
}

impl<E: Event> Listener<E> {
    pub async fn run(&mut self, event: &mut E) -> EventResult<E::ReturnValue> {
        if self.callback.is_none() {
            return Ok(EventState::Continue);
        }

        let callback = self.callback.take().unwrap();
        let state = callback.call(event).await?;

        if !self.once {
            self.callback = Some(callback);
        }

        Ok(state)
    }
}

#[derive(Default)]
pub struct Emitter<E: Event> {
    pub listeners: Vec<Listener<E>>,
}

impl<E: Event> Emitter<E> {
    pub fn on<L: ListenerFunc<E> + 'static>(&mut self, callback: L) -> &mut Self {
        self.listeners.push(Listener {
            callback: Some(Box::new(callback)),
            once: false,
        });

        self
    }

    pub fn once<L: ListenerFunc<E> + 'static>(&mut self, callback: L) -> &mut Self {
        self.listeners.push(Listener {
            callback: Some(Box::new(callback)),
            once: true,
        });

        self
    }

    pub async fn emit(&mut self, mut event: E) -> anyhow::Result<()> {
        for listener in &mut self.listeners {
            listener.run(&mut event).await?;
        }

        Ok(())
    }
}
