use starbase_styles::theme::create_graphical_theme;

pub use miette::*;
pub use thiserror::Error;

pub fn setup_miette() {
    miette::set_panic_hook();

    miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .with_cause_chain()
                .graphical_theme(create_graphical_theme())
                .build(),
        )
    }))
    .unwrap();
}
