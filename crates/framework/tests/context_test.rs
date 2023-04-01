use starship::{ContextManager, Emitter, EventState};
use starship_macros::*;

mod events {
    use super::*;

    #[derive(Debug, Event)]
    struct TestEvent(pub usize);

    #[listener]
    async fn callback_one(event: &mut TestEvent) -> EventResult<TestEvent> {
        event.0 += 5;
        Ok(EventState::Continue)
    }

    #[tokio::test]
    async fn register_and_emit() {
        let mut ctx = ContextManager::default();
        ctx.add_emitter(Emitter::<TestEvent>::new());

        let em = ctx.emitter_mut::<TestEvent>();
        em.listen(CallbackOneListener);

        let (event, _) = em.emit(TestEvent(5)).await.unwrap();

        assert_eq!(event.0, 10);
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
        let mut ctx = ContextManager::default();
        ctx.add_resource(TestResource { field: 5 });

        let resource = ctx.resource::<TestResource>();

        assert_eq!(resource.field, 5);
    }

    #[test]
    fn register_and_write() {
        let mut ctx = ContextManager::default();
        ctx.add_resource(TestResource { field: 5 });

        let resource = ctx.resource_mut::<TestResource>();
        resource.field += 5;

        assert_eq!(resource.field, 10);
    }

    #[test]
    #[should_panic(
        expected = "No resource found for type \"context_test::resources::TestResource\""
    )]
    fn panics_missing_read() {
        let ctx = ContextManager::default();
        ctx.resource::<TestResource>();
    }

    #[test]
    #[should_panic(
        expected = "No resource found for type \"context_test::resources::TestResource\""
    )]
    fn panics_missing_write() {
        let mut ctx = ContextManager::default();
        ctx.resource_mut::<TestResource>();
    }
}

mod state {
    use super::*;

    #[derive(Debug, State)]
    struct TestState(usize);

    #[test]
    fn register_and_read() {
        let mut ctx = ContextManager::default();
        ctx.add_state(TestState(5));

        let state = ctx.state::<TestState>();

        assert_eq!(state.0, 5);
    }

    #[test]
    fn register_and_write() {
        let mut ctx = ContextManager::default();
        ctx.add_state(TestState(5));

        let state = ctx.state_mut::<TestState>();
        (**state) += 5;

        assert_eq!(state.0, 10);
    }

    #[test]
    #[should_panic(expected = "No state found for type \"context_test::state::TestState\"")]
    fn panics_missing_read() {
        let ctx = ContextManager::default();
        ctx.state::<TestState>();
    }

    #[test]
    #[should_panic(expected = "No state found for type \"context_test::state::TestState\"")]
    fn panics_missing_write() {
        let mut ctx = ContextManager::default();
        ctx.state_mut::<TestState>();
    }
}
