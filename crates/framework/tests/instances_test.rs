use starbase::{Emitter, EmitterManager, EventState, ResourceManager, StateManager};
use starbase_macros::*;

mod events {
    use super::*;

    #[derive(Debug, Event)]
    #[event(dataset = usize)]
    struct TestEvent(pub usize);

    #[subscriber]
    async fn callback_one(mut data: TestEvent) -> EventResult {
        *data += 5 + event.0;
        Ok(EventState::Continue)
    }

    #[tokio::test]
    async fn register_and_emit() {
        let ctx = EmitterManager::default();
        ctx.set(Emitter::<TestEvent>::new());

        let mut em = ctx.get::<Emitter<TestEvent>>();
        em.write().on(callback_one).await;

        let data = em.write().emit(TestEvent(5)).await.unwrap();

        assert_eq!(data, 10);
    }
}

mod resources {
    use super::*;

    #[derive(Debug, Resource)]
    struct TestResource {
        pub field: usize,
    }

    #[test]
    fn register_and_read() {
        let ctx = ResourceManager::default();
        ctx.set(TestResource { field: 5 });

        let resource = ctx.get::<TestResource>();

        assert_eq!(resource.read().field, 5);
    }

    #[test]
    fn register_and_write() {
        let ctx = ResourceManager::default();
        ctx.set(TestResource { field: 5 });

        let mut resource = ctx.get::<TestResource>();
        resource.write().field += 5;

        assert_eq!(resource.read().field, 10);
    }

    #[test]
    #[should_panic(expected = "instances_test::resources::TestResource does not exist!")]
    fn panics_missing() {
        let ctx = ResourceManager::default();
        ctx.get::<TestResource>();
    }
}

mod state {
    use super::*;

    #[derive(Debug, State)]
    struct TestState(usize);

    #[test]
    fn register_and_read() {
        let ctx = StateManager::default();
        ctx.set(TestState(5));

        let state = ctx.get::<TestState>();

        assert_eq!(state.read().0, 5);
    }

    #[test]
    fn register_and_write() {
        let ctx = StateManager::default();
        ctx.set(TestState(5));

        let mut state = ctx.get::<TestState>();
        state.write().0 += 5;

        assert_eq!(state.read().0, 10);
    }

    #[test]
    #[should_panic(expected = "instances_test::state::TestState does not exist!")]
    fn panics_missing() {
        let ctx = StateManager::default();
        ctx.get::<TestState>();
    }
}
