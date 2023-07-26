pub trait Event: Send + Sync {
    type Data: Send + Sync + Default;
}

pub enum EventState {
    Continue,
    Stop,
}

pub type EventResult = miette::Result<EventState>;
