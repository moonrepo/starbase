use crate::event::*;
use crate::subscriber::*;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct Emitter<E: Event> {
    subscribers: RwLock<Vec<BoxedSubscriber<E>>>,
}

#[allow(clippy::new_without_default)]
impl<E: Event + 'static> Emitter<E> {
    pub fn new() -> Self {
        Emitter {
            subscribers: RwLock::new(Vec::new()),
        }
    }

    pub async fn len(&self) -> usize {
        self.subscribers.read().await.len()
    }

    pub async fn subscribe<L: Subscriber<E> + 'static>(&self, subscriber: L) -> &Self {
        self.subscribers.write().await.push(Box::new(subscriber));
        self
    }

    pub async fn on<L: SubscriberFunc<E> + 'static>(&self, callback: L) -> &Self {
        self.subscribe(CallbackSubscriber::new(callback, false))
            .await
    }

    pub async fn once<L: SubscriberFunc<E> + 'static>(&self, callback: L) -> &Self {
        self.subscribe(CallbackSubscriber::new(callback, true))
            .await
    }

    pub async fn emit(&self, event: E) -> miette::Result<(E, Option<E::Value>)> {
        let mut result = None;
        let mut remove_indices = HashSet::new();
        let event = Arc::new(RwLock::new(event));
        let mut subscribers = self.subscribers.write().await;

        for (index, subscriber) in subscribers.iter_mut().enumerate() {
            let event = Arc::clone(&event);

            if subscriber.is_once() {
                remove_indices.insert(index);
            }

            match subscriber.on_emit(event).await? {
                EventState::Continue => continue,
                EventState::Stop => break,
                EventState::Return(value) => {
                    result = Some(value);
                    break;
                }
            }
        }

        // Remove only once subscribers that were called
        let mut i = 0;

        subscribers.retain(|_| {
            let remove = remove_indices.contains(&i);
            i += 1;
            !remove
        });

        let event = unsafe { Arc::try_unwrap(event).unwrap_unchecked().into_inner() };

        Ok((event, result))
    }
}
