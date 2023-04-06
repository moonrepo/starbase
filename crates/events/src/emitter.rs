use crate::event::*;
use crate::subscriber::*;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Default)]
pub struct Emitter<E: Event> {
    pub listeners: Vec<BoxedSubscriber<E>>,
}

impl<E: Event + 'static> Emitter<E> {
    pub fn new() -> Self {
        Emitter {
            listeners: Vec::new(),
        }
    }

    pub fn listen<L: Subscriber<E> + 'static>(&mut self, listener: L) -> &mut Self {
        self.listeners.push(Box::new(listener));
        self
    }

    pub fn on<L: SubscriberFunc<E> + 'static>(&mut self, callback: L) -> &mut Self {
        self.listen(CallbackSubscriber::new(callback, false));
        self
    }

    pub fn once<L: SubscriberFunc<E> + 'static>(&mut self, callback: L) -> &mut Self {
        self.listen(CallbackSubscriber::new(callback, true));
        self
    }

    pub async fn emit(&mut self, event: E) -> miette::Result<(E, Option<E::Value>)> {
        let mut result = None;
        let mut remove_indices = HashSet::new();
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
