use core::future::Future;
use starship::{App, Context, ContextManager, Emitter, Event, EventResult, EventState};
use std::{thread::sleep, time::Duration};

#[derive(Debug)]
struct CountEvent(usize);

impl Event for CountEvent {
    type ReturnValue = ();
}

struct One;
struct Two;
struct Three;

async fn test1(ctx: Context) -> anyhow::Result<()> {
    let mut ctx = ctx.write().await;
    println!("init 1");
    // context.state::<One>()?;
    ctx.add_state(One);
    ctx.add_emitter(Emitter::<CountEvent>::new());
    Ok(())
}

async fn e1(event: &mut CountEvent) -> EventResult<CountEvent> {
    println!("emit 1");
    Ok(EventState::Continue)
}

async fn test2(ctx: Context) -> anyhow::Result<()> {
    println!("init 2");
    // context.write().await.state.set(Two);

    let mut ctx = ctx.write().await;
    let em = ctx.emitter_mut::<CountEvent>()?;

    dbg!(&em);

    // em.on(e1);

    em.on(|event: &mut CountEvent| async {
        println!("emit 1");
        Ok(EventState::Continue)
    });

    Ok(())
}

async fn test3(ctx: Context) -> anyhow::Result<()> {
    println!("analyze 1");
    // context.write().await.state.set(Three);
    Ok(())
}

async fn test_system(ctx: Context) -> anyhow::Result<()> {
    {
        ctx.write().await.emit(CountEvent(0)).await?;
    }

    println!("SYSTEM");
    dbg!(ctx.read().await);

    Ok(())
}

#[tokio::main]
async fn main() {
    let mut app = App::default();
    app.add_finalizer(test_system);
    app.add_analyzer(test3);
    app.add_initializer(test1);
    app.add_initializer(test2);

    app.run().await.unwrap();
}
