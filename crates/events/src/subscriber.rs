use crate::event::*;
use async_trait::async_trait;
use std::future::Future;
use std::sync::Arc;
use tokio::sync::RwLock;

#[async_trait]
pub trait Subscriber<E: Event>: Send + Sync {
    fn is_once(&self) -> bool;
    async fn on_emit(&mut self, event: Arc<RwLock<E>>) -> EventResult<E>;
}

pub type BoxedSubscriber<E> = Box<dyn Subscriber<E>>;

#[async_trait]
pub trait SubscriberFunc<E: Event>: Send + Sync {
    async fn call(&self, event: Arc<RwLock<E>>) -> EventResult<E>;
}

#[async_trait]
impl<T: Send + Sync, E: Event + 'static, F> SubscriberFunc<E> for T
where
    T: Fn(Arc<RwLock<E>>) -> F,
    F: Future<Output = EventResult<E>> + Send,
{
    async fn call(&self, event: Arc<RwLock<E>>) -> EventResult<E> {
        self(event).await
    }
}

pub struct CallbackSubscriber<E: Event> {
    func: Box<dyn SubscriberFunc<E>>,
    once: bool,
}

impl<E: Event> CallbackSubscriber<E> {
    pub fn new<F: SubscriberFunc<E> + 'static>(func: F, once: bool) -> Self {
        CallbackSubscriber {
            func: Box::new(func),
            once,
        }
    }
}

#[async_trait]
impl<E: Event> Subscriber<E> for CallbackSubscriber<E> {
    fn is_once(&self) -> bool {
        self.once
    }

    async fn on_emit(&mut self, event: Arc<RwLock<E>>) -> EventResult<E> {
        self.func.call(event).await
    }
}
