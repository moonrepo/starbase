use crate::event::*;
use crate::subscriber::*;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Default)]
pub struct Emitter<E: Event> {
    pub subscribers: Vec<BoxedSubscriber<E>>,
}

impl<E: Event + 'static> Emitter<E> {
    pub fn new() -> Self {
        Emitter {
            subscribers: Vec::new(),
        }
    }

    pub fn subscribe<L: Subscriber<E> + 'static>(&mut self, subscriber: L) -> &mut Self {
        self.subscribers.push(Box::new(subscriber));
        self
    }

    pub fn on<L: SubscriberFunc<E> + 'static>(&mut self, callback: L) -> &mut Self {
        self.subscribe(CallbackSubscriber::new(callback, false));
        self
    }

    pub fn once<L: SubscriberFunc<E> + 'static>(&mut self, callback: L) -> &mut Self {
        self.subscribe(CallbackSubscriber::new(callback, true));
        self
    }

    pub async fn emit(&mut self, event: E) -> miette::Result<(E, Option<E::Value>)> {
        let mut result = None;
        let mut remove_indices = HashSet::new();
        let event = Arc::new(RwLock::new(event));

        for (index, subscriber) in self.subscribers.iter_mut().enumerate() {
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

        self.subscribers.retain(|_| {
            let remove = remove_indices.contains(&i);
            i += 1;
            !remove
        });

        let event = unsafe { Arc::try_unwrap(event).unwrap_unchecked().into_inner() };

        Ok((event, result))
    }
}
