use async_trait::async_trait;
use starbase_events::{Emitter, EventResult, EventState, Subscriber};
use starbase_macros::*;
use std::sync::Arc;
use tokio::sync::RwLock;

mod starbase {
    pub use starbase_events::*;
}

#[derive(Event)]
#[event(dataset = i32)]
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

    async fn on_emit(&mut self, _event: Arc<TestEvent>, data: Arc<RwLock<i32>>) -> EventResult {
        *(data.write().await) += self.inc;
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

    async fn on_emit(&mut self, _event: Arc<TestEvent>, data: Arc<RwLock<i32>>) -> EventResult {
        *(data.write().await) += 3;
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

    async fn on_emit(&mut self, _event: Arc<TestEvent>, data: Arc<RwLock<i32>>) -> EventResult {
        *(data.write().await) += self.inc;
        Ok(EventState::Stop)
    }
}

#[tokio::test]
async fn subscriber() {
    let emitter = Emitter::<TestEvent>::new();
    emitter.subscribe(TestSubscriber { inc: 1 }).await;
    emitter.subscribe(TestSubscriber { inc: 2 }).await;
    emitter.subscribe(TestSubscriber { inc: 3 }).await;

    let data = emitter.emit(TestEvent(0)).await.unwrap();

    assert_eq!(data, 6);
}

#[tokio::test]
async fn subscriber_stop() {
    let emitter = Emitter::<TestEvent>::new();
    emitter.subscribe(TestSubscriber { inc: 1 }).await;
    emitter.subscribe(TestStopSubscriber { inc: 2 }).await;
    emitter.subscribe(TestSubscriber { inc: 3 }).await;

    let data = emitter.emit(TestEvent(0)).await.unwrap();

    assert_eq!(data, 3);
}

#[tokio::test]
async fn subscriber_once() {
    let emitter = Emitter::<TestEvent>::new();
    emitter.subscribe(TestOnceSubscriber).await;
    emitter.subscribe(TestOnceSubscriber).await;
    emitter.subscribe(TestOnceSubscriber).await;

    assert_eq!(emitter.len().await, 3);

    let data = emitter.emit(TestEvent(0)).await.unwrap();

    assert_eq!(data, 9);
    assert_eq!(emitter.len().await, 0);

    let data = emitter.emit(TestEvent(0)).await.unwrap();

    assert_eq!(data, 0);
    assert_eq!(emitter.len().await, 0);
}

// async fn callback_func(event: Arc<RwLock<TestEvent>>) -> EventResult {
//     let mut event = event.write().await;
//     event.0 += 5;
//     Ok(EventState::Continue)
// }

#[subscriber]
async fn callback_one(data: &mut TestEvent) -> EventResult {
    *data += 1;
    Ok(EventState::Continue)
}

#[subscriber]
async fn callback_two(mut data: TestEvent) -> EventResult {
    *data += 2;
    Ok(EventState::Continue)
}

#[subscriber]
async fn callback_three(data: &mut TestEvent) {
    *data += 3;
    Ok(EventState::Continue)
}

#[subscriber]
async fn callback_stop(data: &mut TestEvent) -> EventResult {
    *data += 2;
    Ok(EventState::Stop)
}

#[subscriber]
async fn callback_once(mut data: TestEvent) -> EventResult {
    *data += 3;
    Ok(EventState::Continue)
}

#[tokio::test]
async fn callbacks() {
    let emitter = Emitter::<TestEvent>::new();
    emitter.on(callback_one).await;
    emitter.on(callback_two).await;
    emitter.on(callback_three).await;

    let data = emitter.emit(TestEvent(0)).await.unwrap();

    assert_eq!(data, 6);
}

#[tokio::test]
async fn callbacks_stop() {
    let emitter = Emitter::<TestEvent>::new();
    emitter.on(callback_one).await;
    emitter.on(callback_stop).await;
    emitter.on(callback_three).await;

    let data = emitter.emit(TestEvent(0)).await.unwrap();

    assert_eq!(data, 3);
}

#[tokio::test]
async fn callbacks_once() {
    let emitter = Emitter::<TestEvent>::new();
    emitter.once(callback_once).await;
    emitter.once(callback_once).await;
    emitter.once(callback_once).await;

    assert_eq!(emitter.len().await, 3);

    let data = emitter.emit(TestEvent(0)).await.unwrap();

    assert_eq!(data, 9);
    assert_eq!(emitter.len().await, 0);

    let data = emitter.emit(TestEvent(0)).await.unwrap();

    assert_eq!(data, 0);
    assert_eq!(emitter.len().await, 0);
}

#[tokio::test]
async fn preserves_onces_that_didnt_run() {
    let emitter = Emitter::<TestEvent>::new();
    emitter.once(callback_once).await;
    emitter.once(callback_once).await;
    emitter.on(callback_stop).await;
    emitter.once(callback_once).await;
    emitter.once(callback_once).await;

    assert_eq!(emitter.len().await, 5);

    let data = emitter.emit(TestEvent(0)).await.unwrap();

    assert_eq!(data, 8);
    assert_eq!(emitter.len().await, 3);

    // Will stop immediately
    let data = emitter.emit(TestEvent(0)).await.unwrap();

    assert_eq!(data, 2);
    assert_eq!(emitter.len().await, 3);
}

// #[derive(Event)]
// #[event(dataset = String)]
// struct TestRefEvent<'e> {
//     pub name: &'e str,
//     pub path: &'e Path,
// }

// #[subscriber]
// async fn ref_callback(data: &mut TestRefEvent<'_>) -> EventResult {
//     (*data).push_str(event.name);
//     Ok(EventState::Continue)
// }

// #[tokio::test]
// async fn supports_lifetime_references() {
//     let emitter = Emitter::<TestRefEvent>::new();
//     emitter.on(ref_callback).await;

//     let name = String::from("foo");
//     let path = PathBuf::from("/");
//     let event = TestRefEvent {
//         name: &name,
//         path: &path,
//     };

//     let data = emitter.emit(event).await.unwrap();

//     assert_eq!(data, "foo");
// }
