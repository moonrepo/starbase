use starbase::{Emitter, EmitterManager, EventState, ResourceManager, StateManager};
use starbase_macros::*;

mod events {
    use super::*;

    #[derive(Debug, Event)]
    #[event(dataset = usize)]
    struct TestEvent(pub usize);

    #[subscriber]
    async fn callback_one(mut data: TestEvent) -> EventResult<TestEvent> {
        *data += 5 + event.0;
        Ok(EventState::Continue)
    }

    #[tokio::test]
    async fn register_and_emit() {
        let mut ctx = EmitterManager::default();
        ctx.set(Emitter::<TestEvent>::new());

        let em = ctx.get_mut::<Emitter<TestEvent>>();
        em.on(callback_one).await;

        let (event, data) = em.emit(TestEvent(5)).await.unwrap();

        assert_eq!(event.0, 5);
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
        let mut ctx = ResourceManager::default();
        ctx.set(TestResource { field: 5 });

        let resource = ctx.get::<TestResource>();

        assert_eq!(resource.field, 5);
    }

    #[test]
    fn register_and_write() {
        let mut ctx = ResourceManager::default();
        ctx.set(TestResource { field: 5 });

        let resource = ctx.get_mut::<TestResource>();
        resource.field += 5;

        assert_eq!(resource.field, 10);
    }

    #[test]
    #[should_panic(expected = "instances_test::resources::TestResource does not exist!")]
    fn panics_missing_read() {
        let ctx = ResourceManager::default();
        ctx.get::<TestResource>();
    }

    #[test]
    #[should_panic(expected = "instances_test::resources::TestResource does not exist!")]
    fn panics_missing_write() {
        let mut ctx = ResourceManager::default();
        ctx.get_mut::<TestResource>();
    }
}

mod state {
    use super::*;

    #[derive(Debug, State)]
    struct TestState(usize);

    #[test]
    fn register_and_read() {
        let mut ctx = StateManager::default();
        ctx.set(TestState(5));

        let state = ctx.get::<TestState>();

        assert_eq!(state.0, 5);
    }

    #[test]
    fn register_and_write() {
        let mut ctx = StateManager::default();
        ctx.set(TestState(5));

        let state = ctx.get_mut::<TestState>();
        (**state) += 5;

        assert_eq!(state.0, 10);
    }

    #[test]
    #[should_panic(expected = "instances_test::state::TestState does not exist!")]
    fn panics_missing_read() {
        let ctx = StateManager::default();
        ctx.get::<TestState>();
    }

    #[test]
    #[should_panic(expected = "instances_test::state::TestState does not exist!")]
    fn panics_missing_write() {
        let mut ctx = StateManager::default();
        ctx.get_mut::<TestState>();
    }
}
