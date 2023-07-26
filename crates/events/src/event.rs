pub trait Event: Send + Sync {
    type Data: Send + Sync + Default;
    type ReturnValue;
}

pub enum EventState<V> {
    Continue,
    Stop,
    Return(V),
}

pub type EventResult<E> = miette::Result<EventState<<E as Event>::ReturnValue>>;

pub struct EmitResult<E: Event> {
    pub event: E,
    pub data: E::Data,
    pub value: Option<E::ReturnValue>,
}
