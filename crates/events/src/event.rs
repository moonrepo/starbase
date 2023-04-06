pub trait Event: Send + Sync {
    type Value;
}

pub enum EventState<V> {
    Continue,
    Stop,
    Return(V),
}

pub type EventResult<E> = miette::Result<EventState<<E as Event>::Value>>;
