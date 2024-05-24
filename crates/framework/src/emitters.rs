use crate::create_instance_manager;
use std::any::{type_name, Any, TypeId};
use std::fmt::Debug;
use std::sync::Arc;

pub use starbase_events::{Emitter, Event, EventResult, EventState, Subscriber, SubscriberFunc};

create_instance_manager!(EmitterManager, EmitterInstance);

impl EmitterManager {
    /// Emit the provided event to all registered subscribers. Subscribers will be
    /// called in the order they were registered.
    ///
    /// If a subscriber returns [`EventState::Stop`], no further subscribers will be called.
    /// If a subscriber returns [`EventState::Return`], no further subscribers will be called
    /// and the provided value will be returned.
    /// If a subscriber returns [`EventState::Continue`], the next subscriber will be called.
    ///
    /// When complete, the provided event will be returned along with the value returned
    /// by the subscriber that returned [`EventState::Return`], or [`None`] if not occurred.
    pub async fn emit<E: Event + 'static>(&self, event: E) -> miette::Result<E::Data> {
        self.get::<Emitter<E>>().await.read().emit(event).await
    }
}

impl<E: Event + 'static> EmitterInstance for Emitter<E> {}

pub type Emitters = Arc<EmitterManager>;
