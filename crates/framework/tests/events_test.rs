use async_trait::async_trait;
use starbase::{Emitter, EventResult, EventState, Listener};
use starbase_macros::*;

#[derive(Event)]
#[event(value = "i32")]
struct TestEvent(pub i32);

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

// async fn callback_func(event: &mut TestEvent) -> EventResult<TestEvent> {
//     event.0 += 1;
//     Ok(EventState::Continue)
// }

#[listener]
async fn callback_one(event: &mut TestEvent) -> EventResult<TestEvent> {
    event.0 += 1;
    Ok(EventState::Continue)
}

#[listener]
async fn callback_two(event: &mut TestEvent) -> EventResult<TestEvent> {
    event.0 += 2;
    Ok(EventState::Continue)
}

#[listener]
async fn callback_three(event: &mut TestEvent) -> EventResult<TestEvent> {
    event.0 += 3;
    Ok(EventState::Continue)
}

#[listener]
async fn callback_return(event: &mut TestEvent) -> EventResult<TestEvent> {
    Ok(EventState::Return(0))
}

#[listener]
async fn callback_stop(event: &mut TestEvent) -> EventResult<TestEvent> {
    event.0 += 2;
    Ok(EventState::Stop)
}

#[listener(once)]
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
    // emitter.on(callback_func);
    // emitter.on(callback_func);
    // emitter.on(callback_func);

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
