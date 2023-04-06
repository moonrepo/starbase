use crate::create_instance_manager;
use async_trait::async_trait;
use core::future::Future;
use rustc_hash::{FxHashMap, FxHashSet};
use std::any::{type_name, Any, TypeId};
use std::fmt::Debug;
use std::sync::Arc;
use tokio::sync::RwLock;

pub type EventResult<E> = miette::Result<EventState<<E as Event>::Value>>;

pub trait Event: Send + Sync {
    type Value;
}

pub enum EventState<V> {
    Continue,
    Stop,
    Return(V),
}

#[async_trait]
pub trait Listener<E: Event>: Send + Sync {
    fn is_once(&self) -> bool;
    async fn on_emit(&mut self, event: Arc<RwLock<E>>) -> EventResult<E>;
}

pub type BoxedListener<E> = Box<dyn Listener<E>>;

#[async_trait]
pub trait ListenerFunc<E: Event>: Send + Sync {
    async fn call(&self, event: Arc<RwLock<E>>) -> EventResult<E>;
}

#[async_trait]
impl<T: Send + Sync, E: Event + 'static, F> ListenerFunc<E> for T
where
    T: Fn(Arc<RwLock<E>>) -> F,
    F: Future<Output = EventResult<E>> + Send,
{
    async fn call(&self, event: Arc<RwLock<E>>) -> EventResult<E> {
        self(event).await
    }
}

pub struct CallbackListener<E: Event> {
    func: Box<dyn ListenerFunc<E>>,
    once: bool,
}

#[async_trait]
impl<E: Event> Listener<E> for CallbackListener<E> {
    fn is_once(&self) -> bool {
        self.once
    }

    async fn on_emit(&mut self, event: Arc<RwLock<E>>) -> EventResult<E> {
        self.func.call(event).await
    }
}

#[derive(Default)]
pub struct Emitter<E: Event> {
    pub listeners: Vec<BoxedListener<E>>,
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
            func: Box::new(callback),
            once: false,
        }));

        self
    }

    pub fn once<L: ListenerFunc<E> + 'static>(&mut self, callback: L) -> &mut Self {
        self.listeners.push(Box::new(CallbackListener {
            func: Box::new(callback),
            once: true,
        }));

        self
    }

    pub async fn emit(&mut self, event: E) -> miette::Result<(E, Option<E::Value>)> {
        let mut result = None;
        let mut remove_indices = FxHashSet::default();
        let event = Arc::new(RwLock::new(event));

        for (index, listener) in self.listeners.iter_mut().enumerate() {
            let event = Arc::clone(&event);

            if listener.is_once() {
                remove_indices.insert(index);
            }

            match listener.on_emit(event).await? {
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

        let event = unsafe { Arc::try_unwrap(event).unwrap_unchecked().into_inner() };

        Ok((event, result))
    }
}

create_instance_manager!(EmitterManager, EmitterInstance);

impl EmitterManager {
    pub async fn emit<E: Event + 'static>(
        &mut self,
        event: E,
    ) -> miette::Result<(E, Option<E::Value>)> {
        self.get_mut::<Emitter<E>>().emit(event).await
    }
}

impl<E: Event + 'static> EmitterInstance for Emitter<E> {}

pub type Emitters = Arc<RwLock<EmitterManager>>;
