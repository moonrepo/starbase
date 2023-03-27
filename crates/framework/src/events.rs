use async_trait::async_trait;
use core::future::Future;
use futures::future::BoxFuture;
use rustc_hash::FxHashSet;
use std::fmt::Debug;

pub type EventResult<E> = anyhow::Result<EventState<<E as Event>::Value>>;
pub type EventFutureResult<E> = BoxFuture<'static, EventResult<E>>;

pub trait Event: Send + Sync {
    type Value;
}

pub enum EventState<V> {
    Continue,
    Stop,
    Return(V),
}

#[async_trait]
pub trait ListenerFunc<E: Event>: Send + Sync {
    async fn call(&self, event: &mut E) -> EventResult<E>;
}

#[async_trait]
impl<T: Send + Sync, F, E: Event> ListenerFunc<E> for T
where
    T: Fn(&mut E) -> F,
    F: Future<Output = EventResult<E>> + Send + 'static,
{
    async fn call(&self, event: &mut E) -> EventResult<E> {
        Ok(self(event).await?)
    }
}

#[async_trait]
pub trait Listener<E: Event>: Debug + Send + Sync {
    fn is_once(&self) -> bool;
    async fn on_emit(&mut self, event: &mut E) -> EventResult<E>;
}

pub struct CallbackListener<E: Event> {
    callback: Box<dyn ListenerFunc<E>>,
    once: bool,
}

#[async_trait]
impl<E: Event> Listener<E> for CallbackListener<E> {
    fn is_once(&self) -> bool {
        self.once
    }

    async fn on_emit(&mut self, event: &mut E) -> EventResult<E> {
        Ok(self.callback.call(event).await?)
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
    pub listeners: Vec<Box<dyn Listener<E>>>,
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
            callback: Box::new(callback),
            once: false,
        }));

        self
    }

    pub fn once<L: ListenerFunc<E> + 'static>(&mut self, callback: L) -> &mut Self {
        self.listeners.push(Box::new(CallbackListener {
            callback: Box::new(callback),
            once: true,
        }));

        self
    }

    pub async fn emit(&mut self, mut event: E) -> anyhow::Result<(E, Option<E::Value>)> {
        let mut result = None;
        let mut remove_indices = FxHashSet::default();

        for (index, listener) in self.listeners.iter_mut().enumerate() {
            if listener.is_once() {
                remove_indices.insert(index);
            }

            match listener.on_emit(&mut event).await? {
                EventState::Continue => continue,
                EventState::Stop => break,
                EventState::Return(value) => {
                    result = Some(value);
                    break;
                }
            }
        }

        // Remove only once listeners that were called
        let mut i = 0;

        self.listeners.retain(|_| {
            let remove = remove_indices.contains(&i);
            i += 1;
            !remove
        });

        Ok((event, result))
    }
}
