use crate::event::*;
use crate::subscriber::*;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct Emitter<E: Event> {
    subscribers: Arc<RwLock<Vec<BoxedSubscriber<E>>>>,
}

#[allow(clippy::new_without_default, clippy::len_without_is_empty)]
impl<E: Event + 'static> Emitter<E> {
    /// Create a new event emitter.
    pub fn new() -> Self {
        Emitter {
            subscribers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Return a count of how many subscribers have been registered.
    pub async fn len(&self) -> usize {
        self.subscribers.read().await.len()
    }

    /// Register a subscriber to receive events.
    pub async fn subscribe<L: Subscriber<E> + 'static>(&self, subscriber: L) -> &Self {
        self.subscribers.write().await.push(Box::new(subscriber));
        self
    }

    /// Register a subscriber function to receive events.
    pub async fn on<L: SubscriberFunc<E> + 'static>(&self, callback: L) -> &Self {
        self.subscribe(CallbackSubscriber::new(callback, false))
            .await
    }

    /// Register a subscriber function that will unregister itself after the first
    /// event is received. This is useful for one-time event handlers.
    pub async fn once<L: SubscriberFunc<E> + 'static>(&self, callback: L) -> &Self {
        self.subscribe(CallbackSubscriber::new(callback, true))
            .await
    }

    /// Emit the provided event to all registered subscribers. Subscribers will be
    /// called in the order they were registered.
    ///
    /// If a subscriber returns [EventState::Stop], no further subscribers will be called.
    /// If a subscriber returns [EventState::Return], no further subscribers will be called
    /// and the provided value will be returned.
    /// If a subscriber returns [EventState::Continue], the next subscriber will be called.
    ///
    /// When complete, the provided event will be returned along with the value returned
    /// by the subscriber that returned [EventState::Return], or [None] if not occurred.
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
