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

#[async_trait]
pub trait ListenerFunc<E: Event>: Send + Sync {
    async fn call(&self, event: &mut E) -> EventResult<E>;
}

#[async_trait]
impl<T: Send + Sync, F, E: Event> ListenerFunc<E> for T
where
    T: Fn(&mut E) -> F,
    F: Future<Output = EventResult<E>> + Send + 'static,
{
    async fn call(&self, event: &mut E) -> EventResult<E> {
        Ok(self(event).await?)
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
    use starship_macros::listener;

    #[derive(Debug)]
    struct TestEvent(pub i32);

    impl Event for TestEvent {
        type Value = i32;
    }

    #[derive(Debug)]
    struct TestListener {
        inc: i32,
    }

    #[async_trait]
    impl Listener<TestEvent> for TestListener {
        async fn on_emit(&mut self, event: &mut TestEvent) -> EventResult<TestEvent> {
            event.0 += self.inc;
            Ok(EventState::Continue)
        }
    }

    #[derive(Debug)]
    struct TestStopListener {
        inc: i32,
    }

    #[async_trait]
    impl Listener<TestEvent> for TestStopListener {
        async fn on_emit(&mut self, event: &mut TestEvent) -> EventResult<TestEvent> {
            event.0 += self.inc;
            Ok(EventState::Stop)
        }
    }

    #[derive(Debug)]
    struct TestReturnListener;

    #[async_trait]
    impl Listener<TestEvent> for TestReturnListener {
        async fn on_emit(&mut self, _event: &mut TestEvent) -> EventResult<TestEvent> {
            Ok(EventState::Return(0))
        }
    }

    #[tokio::test]
    async fn listener() {
        let mut emitter = Emitter::<TestEvent>::new();
        emitter.listen(TestListener { inc: 1 });
        emitter.listen(TestListener { inc: 2 });
        emitter.listen(TestListener { inc: 3 });

        let (event, result) = emitter.emit(TestEvent(0)).await.unwrap();

        assert_eq!(event.0, 6);
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn listener_return() {
        let mut emitter = Emitter::<TestEvent>::new();
        emitter.listen(TestListener { inc: 1 });
        emitter.listen(TestListener { inc: 2 });
        emitter.listen(TestReturnListener);

        let (event, result) = emitter.emit(TestEvent(0)).await.unwrap();

        assert_eq!(event.0, 3);
        assert_eq!(result, Some(0));
    }

    #[tokio::test]
    async fn listener_stop() {
        let mut emitter = Emitter::<TestEvent>::new();
        emitter.listen(TestListener { inc: 1 });
        emitter.listen(TestStopListener { inc: 2 });
        emitter.listen(TestListener { inc: 3 });

        let (event, result) = emitter.emit(TestEvent(0)).await.unwrap();

        assert_eq!(event.0, 3);
        assert_eq!(result, None);
    }

    #[listener(local)]
    async fn callback_one(event: &mut TestEvent) -> EventResult<TestEvent> {
        event.0 += 1;
        Ok(EventState::Continue)
    }

    #[listener(local)]
    async fn callback_two(event: &mut TestEvent) -> EventResult<TestEvent> {
        event.0 += 2;
        Ok(EventState::Continue)
    }

    #[listener(local)]
    async fn callback_three(event: &mut TestEvent) -> EventResult<TestEvent> {
        event.0 += 3;
        Ok(EventState::Continue)
    }

    #[tokio::test]
    async fn callback() {
        let mut emitter = Emitter::<TestEvent>::new();
        emitter.listen(CallbackOneListener);
        emitter.listen(CallbackTwoListener);
        emitter.listen(CallbackThreeListener);
        // emitter.on(callback_one);
        // emitter.on(callback_two);
        // emitter.on(callback_three);

        let (event, result) = emitter.emit(TestEvent(0)).await.unwrap();

        assert_eq!(event.0, 6);
        assert_eq!(result, None);
    }
}
