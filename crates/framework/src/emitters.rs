use crate::create_instance_manager;
use rustc_hash::FxHashMap;
use std::any::{type_name, Any, TypeId};
use std::fmt::Debug;
use std::sync::Arc;
use tokio::sync::RwLock;

pub use starbase_events::{Emitter, Event, EventResult, EventState, Subscriber, SubscriberFunc};

create_instance_manager!(EmitterManager, EmitterInstance);

impl EmitterManager {
    pub async fn emit<E: Event + 'static>(
        &self,
        event: E,
    ) -> miette::Result<(E, Option<E::Value>)> {
        self.get::<Emitter<E>>().emit(event).await
    }
}

impl<E: Event + 'static> EmitterInstance for Emitter<E> {}

pub type Emitters = Arc<RwLock<EmitterManager>>;
