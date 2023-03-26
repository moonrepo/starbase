use async_trait::async_trait;
use core::future::Future;
use futures::future::BoxFuture;
use rustc_hash::FxHashSet;
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
    fn is_once(&self) -> bool;
    async fn on_emit(&mut self, event: &mut E) -> EventResult<E>;
}

pub struct CallbackListener<E: Event> {
    callback: Box<dyn ListenerFunc<E>>,
    once: bool,
}

#[async_trait]
impl<E: Event> Listener<E> for CallbackListener<E> {
    fn is_once(&self) -> bool {
        self.once
    }

    async fn on_emit(&mut self, event: &mut E) -> EventResult<E> {
        Ok(self.callback.call(event).await?)
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
    pub listeners: Vec<Box<dyn Listener<E>>>,
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
            callback: Box::new(callback),
            once: false,
        }));

        self
    }

    pub fn once<L: ListenerFunc<E> + 'static>(&mut self, callback: L) -> &mut Self {
        self.listeners.push(Box::new(CallbackListener {
            callback: Box::new(callback),
            once: true,
        }));

        self
    }

    pub async fn emit(&mut self, mut event: E) -> anyhow::Result<(E, Option<E::Value>)> {
        let mut result = None;
        let mut remove_indices = FxHashSet::default();

        for (index, listener) in self.listeners.iter_mut().enumerate() {
            if listener.is_once() {
                remove_indices.insert(index);
            }

            match listener.on_emit(&mut event).await? {
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

        Ok((event, result))
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
        fn is_once(&self) -> bool {
            false
        }

        async fn on_emit(&mut self, event: &mut TestEvent) -> EventResult<TestEvent> {
            event.0 += self.inc;
            Ok(EventState::Continue)
        }
    }

    #[derive(Debug)]
    struct TestOnceListener;

    #[async_trait]
    impl Listener<TestEvent> for TestOnceListener {
        fn is_once(&self) -> bool {
            true
        }

        async fn on_emit(&mut self, event: &mut TestEvent) -> EventResult<TestEvent> {
            event.0 += 3;
            Ok(EventState::Continue)
        }
    }

    #[derive(Debug)]
    struct TestStopListener {
        inc: i32,
    }

    #[async_trait]
    impl Listener<TestEvent> for TestStopListener {
        fn is_once(&self) -> bool {
            false
        }

        async fn on_emit(&mut self, event: &mut TestEvent) -> EventResult<TestEvent> {
            event.0 += self.inc;
            Ok(EventState::Stop)
        }
    }

    #[derive(Debug)]
    struct TestReturnListener;

    #[async_trait]
    impl Listener<TestEvent> for TestReturnListener {
        fn is_once(&self) -> bool {
            false
        }

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

    #[tokio::test]
    async fn listener_once() {
        let mut emitter = Emitter::<TestEvent>::new();
        emitter.listen(TestOnceListener);
        emitter.listen(TestOnceListener);
        emitter.listen(TestOnceListener);

        assert_eq!(emitter.listeners.len(), 3);

        let (event, result) = emitter.emit(TestEvent(0)).await.unwrap();

        assert_eq!(event.0, 9);
        assert_eq!(result, None);
        assert_eq!(emitter.listeners.len(), 0);

        let (event, result) = emitter.emit(TestEvent(0)).await.unwrap();

        assert_eq!(event.0, 0);
        assert_eq!(result, None);
        assert_eq!(emitter.listeners.len(), 0);
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

    #[listener(local)]
    async fn callback_return(event: &mut TestEvent) -> EventResult<TestEvent> {
        Ok(EventState::Return(0))
    }

    #[listener(local)]
    async fn callback_stop(event: &mut TestEvent) -> EventResult<TestEvent> {
        event.0 += 2;
        Ok(EventState::Stop)
    }

    #[listener(local, once)]
    async fn callback_once(event: &mut TestEvent) -> EventResult<TestEvent> {
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

    #[tokio::test]
    async fn callback_return() {
        let mut emitter = Emitter::<TestEvent>::new();
        emitter.listen(CallbackOneListener);
        emitter.listen(CallbackTwoListener);
        emitter.listen(CallbackReturnListener);

        let (event, result) = emitter.emit(TestEvent(0)).await.unwrap();

        assert_eq!(event.0, 3);
        assert_eq!(result, Some(0));
    }

    #[tokio::test]
    async fn callback_stop() {
        let mut emitter = Emitter::<TestEvent>::new();
        emitter.listen(CallbackOneListener);
        emitter.listen(CallbackStopListener);
        emitter.listen(CallbackThreeListener);

        let (event, result) = emitter.emit(TestEvent(0)).await.unwrap();

        assert_eq!(event.0, 3);
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn callback_once() {
        let mut emitter = Emitter::<TestEvent>::new();
        emitter.listen(CallbackOnceListener);
        emitter.listen(CallbackOnceListener);
        emitter.listen(CallbackOnceListener);

        assert_eq!(emitter.listeners.len(), 3);

        let (event, result) = emitter.emit(TestEvent(0)).await.unwrap();

        assert_eq!(event.0, 9);
        assert_eq!(result, None);
        assert_eq!(emitter.listeners.len(), 0);

        let (event, result) = emitter.emit(TestEvent(0)).await.unwrap();

        assert_eq!(event.0, 0);
        assert_eq!(result, None);
        assert_eq!(emitter.listeners.len(), 0);
    }

    #[tokio::test]
    async fn preserves_onces_that_didnt_run() {
        let mut emitter = Emitter::<TestEvent>::new();
        emitter.listen(CallbackOnceListener);
        emitter.listen(CallbackOnceListener);
        emitter.listen(CallbackStopListener);
        emitter.listen(CallbackOnceListener);
        emitter.listen(CallbackOnceListener);

        assert_eq!(emitter.listeners.len(), 5);

        let (event, result) = emitter.emit(TestEvent(0)).await.unwrap();

        assert_eq!(event.0, 8);
        assert_eq!(result, None);
        assert_eq!(emitter.listeners.len(), 3);

        // Will stop immediately
        let (event, result) = emitter.emit(TestEvent(0)).await.unwrap();

        assert_eq!(event.0, 2);
        assert_eq!(result, None);
        assert_eq!(emitter.listeners.len(), 3);
    }
}
