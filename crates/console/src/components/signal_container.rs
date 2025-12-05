use super::layout::Container;
use iocraft::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};

pub static INTERRUPTED: AtomicBool = AtomicBool::new(false);

pub fn received_interrupt_signal() -> bool {
    INTERRUPTED.load(Ordering::Relaxed)
}

#[derive(Default, Props)]
pub struct SignalContainerProps<'a> {
    pub children: Vec<AnyElement<'a>>,
}

#[component]
pub fn SignalContainer<'a>(
    props: &mut SignalContainerProps<'a>,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'a>> {
    let mut system = hooks.use_context_mut::<SystemContext>();
    let mut should_exit = hooks.use_state(|| false);

    hooks.use_terminal_events({
        move |event| {
            if let TerminalEvent::Key(KeyEvent {
                code,
                kind,
                modifiers,
                ..
            }) = event
            {
                if kind != KeyEventKind::Release
                    && modifiers == KeyModifiers::CONTROL
                    && code == KeyCode::Char('c')
                {
                    should_exit.set(true)
                }
            }
        }
    });

    if should_exit.get() {
        INTERRUPTED.store(true, Ordering::Release);
        system.exit();

        return element!(View).into_any();
    }

    element! {
        Container {
            #(&mut props.children)
        }
    }
    .into_any()
}
