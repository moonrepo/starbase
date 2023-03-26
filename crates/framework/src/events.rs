use async_trait::async_trait;
use core::future::Future;
use futures::future::BoxFuture;
use std::fmt::Debug;

pub type EventResult<E> = anyhow::Result<EventState<<E as Event>::Value>>;
pub type EventFutureResult<E> = BoxFuture<'static, EventResult<E>>;

pub trait Event: Send + Sync {
    type Value;
}

pub enum EventState<V> {
    Continue,
    Stop,
    Return(V),
}

pub trait ListenerFunc<E: Event>: Send + Sync {
    fn call(&self, event: &mut E) -> EventFutureResult<E>;
}

impl<T: Send + Sync, F, E: Event> ListenerFunc<E> for T
where
    T: Fn(&mut E) -> F,
    F: Future<Output = EventResult<E>> + Send + 'static,
{
    fn call(&self, event: &mut E) -> EventFutureResult<E> {
        Box::pin(self(event))
    }
}

#[async_trait]
pub trait Listener<E: Event>: Debug + Send + Sync {
    async fn on_emit(&mut self, event: &mut E) -> EventResult<E>;
}

pub struct CallbackListener<E: Event> {
    callback: Option<Box<dyn ListenerFunc<E>>>,
    once: bool,
}

#[async_trait]
impl<E: Event> Listener<E> for CallbackListener<E> {
    async fn on_emit(&mut self, event: &mut E) -> EventResult<E> {
        if self.callback.is_none() {
            return Ok(EventState::Continue);
        }

        let callback = self.callback.take().unwrap();
        let state = callback.call(event).await?;

        if !self.once {
            self.callback = Some(callback);
        }

        Ok(state)
    }
}

impl<E: Event> Debug for CallbackListener<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(if self.once {
            "CallbackListener(once)"
        } else {
            "CallbackListener"
        })
        .finish()
    }
}

#[derive(Debug, Default)]
pub struct Emitter<E: Event> {
    listeners: Vec<Box<dyn Listener<E>>>,
}

impl<E: Event + 'static> Emitter<E> {
    pub fn new() -> Self {
        Emitter {
            listeners: Vec::new(),
        }
    }

    pub fn listen<L: Listener<E> + 'static>(&mut self, listener: L) -> &mut Self {
        self.listeners.push(Box::new(listener));
        self
    }

    pub fn on<L: ListenerFunc<E> + 'static>(&mut self, callback: L) -> &mut Self {
        self.listeners.push(Box::new(CallbackListener {
            callback: Some(Box::new(callback)),
            once: false,
        }));

        self
    }

    pub fn once<L: ListenerFunc<E> + 'static>(&mut self, callback: L) -> &mut Self {
        self.listeners.push(Box::new(CallbackListener {
            callback: Some(Box::new(callback)),
            once: true,
        }));

        self
    }

    pub async fn emit(&mut self, mut event: E) -> anyhow::Result<(E, Option<E::Value>)> {
        for listener in &mut self.listeners {
            match listener.on_emit(&mut event).await? {
                EventState::Continue => {}
                EventState::Stop => break,
                EventState::Return(value) => {
                    return Ok((event, Some(value)));
                }
            }
        }

        Ok((event, None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct TestEvent(pub i32);

    impl Event for TestEvent {
        type Value = i32;
    }

    #[derive(Debug)]
    struct TestListener {
        value: i32,
    }

    #[async_trait]
    impl Listener<TestEvent> for TestListener {
        async fn on_emit(&mut self, event: &mut TestEvent) -> EventResult<TestEvent> {
            event.0 += self.value;
            Ok(EventState::Continue)
        }
    }

    #[tokio::test]
    async fn test_event() {
        let mut emitter = Emitter::<TestEvent>::new();
        emitter.listen(TestListener { value: 1 });
        emitter.listen(TestListener { value: 2 });
        emitter.listen(TestListener { value: 3 });

        let (event, _) = emitter.emit(TestEvent(0)).await.unwrap();

        assert_eq!(event.0, 6);
    }

    // #[tokio::test]
    // async fn test_event_return() {
    //     let mut emitter = Emitter::<TestEvent>::new();
    //     emitter.listen(TestListener { value: 1 });
    //     emitter.listen(TestListener { value: 2 });
    //     emitter.listen(TestListener { value: 3 });

    //     let mut event = TestEvent { value: 0 };
    //     let value = emitter.emit(event).await.unwrap().unwrap();

    //     assert_eq!(value, 6);
    // }

    // #[tokio::test]
    // async fn test_event_stop() {
    //     let mut emitter = Emitter::<TestEvent>::new();
    //     emitter.listen(TestListener { value: 1 });
    //     emitter.listen(TestListener { value: 2 });
    //     emitter.listen(TestListener { value: 3 });

    //     let mut event = TestEvent { value: 0 };
    //     emitter.emit(event).await.unwrap();

    //     assert_eq!(event.value, 6);
    // }

    // #[tokio::test]
    // async fn test_event_callback() {
    //     let mut emitter = Emitter::<TestEvent>::new();
    //     emitter.on(|event: &mut TestEvent| async move {
    //         event.value += 1;
    //         Ok(EventState::Continue)
    //     });
    //     emitter.on(|event: &mut TestEvent| async move {
    //         event.value += 2;
    //         Ok(EventState::Continue)
    //     });
    // }
}
