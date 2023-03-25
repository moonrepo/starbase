use async_trait::async_trait;
use core::future::Future;
use futures::future::BoxFuture;
use std::fmt::Debug;

pub type EventResult<E> = anyhow::Result<EventState<<E as Event>::ReturnValue>>;
pub type EventFutureResult<E> = BoxFuture<'static, EventResult<E>>;

pub trait Event: Send + Sync {
    type ReturnValue;
}

pub enum EventState<R> {
    Continue,
    Stop,
    Return(R),
}

pub trait ListenerFunc<E: Event>: Send + Sync {
    fn call(&self, event: &mut E) -> EventFutureResult<E>;
}

impl<T: Send + Sync, F, E: Event> ListenerFunc<E> for T
where
    T: Fn(&mut E) -> F,
    F: Future<Output = EventResult<E>> + Send + 'static,
{
    fn call(&self, event: &mut E) -> EventFutureResult<E> {
        Box::pin(self(event))
    }
}

#[async_trait]
pub trait Listener<E: Event>: Debug + Send + Sync {
    async fn on_emit(&mut self, event: &mut E) -> EventResult<E>;
}

pub struct CallbackListener<E: Event> {
    callback: Option<Box<dyn ListenerFunc<E>>>,
    once: bool,
}

#[async_trait]
impl<E: Event> Listener<E> for CallbackListener<E> {
    async fn on_emit(&mut self, event: &mut E) -> EventResult<E> {
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

impl<E: Event> Debug for CallbackListener<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(if self.once {
            "CallbackListener(once)"
        } else {
            "CallbackListener"
        })
        .finish()
    }
}

#[derive(Debug, Default)]
pub struct Emitter<E: Event> {
    listeners: Vec<Box<dyn Listener<E>>>,
}

impl<E: Event + 'static> Emitter<E> {
    pub fn new() -> Self {
        Emitter {
            listeners: Vec::new(),
        }
    }

    pub fn listen<L: Listener<E> + 'static>(&mut self, listener: L) -> &mut Self {
        self.listeners.push(Box::new(listener));
        self
    }

    pub fn on<L: ListenerFunc<E> + 'static>(&mut self, callback: L) -> &mut Self {
        self.listeners.push(Box::new(CallbackListener {
            callback: Some(Box::new(callback)),
            once: false,
        }));

        self
    }

    pub fn once<L: ListenerFunc<E> + 'static>(&mut self, callback: L) -> &mut Self {
        self.listeners.push(Box::new(CallbackListener {
            callback: Some(Box::new(callback)),
            once: true,
        }));

        self
    }

    pub async fn emit(&mut self, mut event: E) -> anyhow::Result<Option<E::ReturnValue>> {
        for listener in &mut self.listeners {
            match listener.on_emit(&mut event).await? {
                EventState::Continue => {}
                EventState::Stop => break,
                EventState::Return(value) => {
                    return Ok(Some(value));
                }
            }
        }

        Ok(None)
    }
}
