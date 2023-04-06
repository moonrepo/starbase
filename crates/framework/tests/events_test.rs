use async_trait::async_trait;
use starbase::{Emitter, EventResult, EventState, Subscriber};
use starbase_macros::*;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Event)]
#[event(value = "i32")]
struct TestEvent(pub i32);

#[derive(Debug)]
struct TestSubscriber {
    inc: i32,
}

#[async_trait]
impl Subscriber<TestEvent> for TestSubscriber {
    fn is_once(&self) -> bool {
        false
    }

    async fn on_emit(&mut self, event: Arc<RwLock<TestEvent>>) -> EventResult<TestEvent> {
        event.write().await.0 += self.inc;
        Ok(EventState::Continue)
    }
}

#[derive(Debug)]
struct TestOnceSubscriber;

#[async_trait]
impl Subscriber<TestEvent> for TestOnceSubscriber {
    fn is_once(&self) -> bool {
        true
    }

    async fn on_emit(&mut self, event: Arc<RwLock<TestEvent>>) -> EventResult<TestEvent> {
        event.write().await.0 += 3;
        Ok(EventState::Continue)
    }
}

#[derive(Debug)]
struct TestStopSubscriber {
    inc: i32,
}

#[async_trait]
impl Subscriber<TestEvent> for TestStopSubscriber {
    fn is_once(&self) -> bool {
        false
    }

    async fn on_emit(&mut self, event: Arc<RwLock<TestEvent>>) -> EventResult<TestEvent> {
        event.write().await.0 += self.inc;
        Ok(EventState::Stop)
    }
}

#[derive(Debug)]
struct TestReturnSubscriber;

#[async_trait]
impl Subscriber<TestEvent> for TestReturnSubscriber {
    fn is_once(&self) -> bool {
        false
    }

    async fn on_emit(&mut self, _event: Arc<RwLock<TestEvent>>) -> EventResult<TestEvent> {
        Ok(EventState::Return(0))
    }
}

#[tokio::test]
async fn listener() {
    let mut emitter = Emitter::<TestEvent>::new();
    emitter.listen(TestSubscriber { inc: 1 });
    emitter.listen(TestSubscriber { inc: 2 });
    emitter.listen(TestSubscriber { inc: 3 });

    let (event, result) = emitter.emit(TestEvent(0)).await.unwrap();

    assert_eq!(event.0, 6);
    assert_eq!(result, None);
}

#[tokio::test]
async fn listener_return() {
    let mut emitter = Emitter::<TestEvent>::new();
    emitter.listen(TestSubscriber { inc: 1 });
    emitter.listen(TestSubscriber { inc: 2 });
    emitter.listen(TestReturnSubscriber);

    let (event, result) = emitter.emit(TestEvent(0)).await.unwrap();

    assert_eq!(event.0, 3);
    assert_eq!(result, Some(0));
}

#[tokio::test]
async fn listener_stop() {
    let mut emitter = Emitter::<TestEvent>::new();
    emitter.listen(TestSubscriber { inc: 1 });
    emitter.listen(TestStopSubscriber { inc: 2 });
    emitter.listen(TestSubscriber { inc: 3 });

    let (event, result) = emitter.emit(TestEvent(0)).await.unwrap();

    assert_eq!(event.0, 3);
    assert_eq!(result, None);
}

#[tokio::test]
async fn listener_once() {
    let mut emitter = Emitter::<TestEvent>::new();
    emitter.listen(TestOnceSubscriber);
    emitter.listen(TestOnceSubscriber);
    emitter.listen(TestOnceSubscriber);

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

// async fn callback_func(event: Arc<RwLock<TestEvent>>) -> EventResult<TestEvent> {
//     let mut event = event.write().await;
//     event.0 += 5;
//     Ok(EventState::Continue)
// }

#[listener]
async fn callback_one(event: &mut TestEvent) -> EventResult<TestEvent> {
    event.0 += 1;
    Ok(EventState::Continue)
}

#[listener]
async fn callback_two(mut event: TestEvent) -> EventResult<TestEvent> {
    event.0 += 2;
    Ok(EventState::Continue)
}

#[listener]
async fn callback_three(event: &mut TestEvent) {
    event.0 += 3;
    Ok(EventState::Continue)
}

#[listener]
async fn callback_return(_event: TestEvent) {
    Ok(EventState::Return(0))
}

#[listener]
async fn callback_stop(event: &mut TestEvent) -> EventResult<TestEvent> {
    event.0 += 2;
    Ok(EventState::Stop)
}

#[listener]
async fn callback_once(mut event: TestEvent) -> EventResult<TestEvent> {
    event.0 += 3;
    Ok(EventState::Continue)
}

#[tokio::test]
async fn callbacks() {
    let mut emitter = Emitter::<TestEvent>::new();
    emitter.on(callback_one);
    emitter.on(callback_two);
    emitter.on(callback_three);

    let (event, result) = emitter.emit(TestEvent(0)).await.unwrap();

    assert_eq!(event.0, 6);
    assert_eq!(result, None);
}

#[tokio::test]
async fn callbacks_return() {
    let mut emitter = Emitter::<TestEvent>::new();
    emitter.on(callback_one);
    emitter.on(callback_two);
    emitter.on(callback_return);

    let (event, result) = emitter.emit(TestEvent(0)).await.unwrap();

    assert_eq!(event.0, 3);
    assert_eq!(result, Some(0));
}

#[tokio::test]
async fn callbacks_stop() {
    let mut emitter = Emitter::<TestEvent>::new();
    emitter.on(callback_one);
    emitter.on(callback_stop);
    emitter.on(callback_three);

    let (event, result) = emitter.emit(TestEvent(0)).await.unwrap();

    assert_eq!(event.0, 3);
    assert_eq!(result, None);
}

#[tokio::test]
async fn callbacks_once() {
    let mut emitter = Emitter::<TestEvent>::new();
    emitter.once(callback_once);
    emitter.once(callback_once);
    emitter.once(callback_once);

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
    emitter.once(callback_once);
    emitter.once(callback_once);
    emitter.on(callback_stop);
    emitter.once(callback_once);
    emitter.once(callback_once);

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
