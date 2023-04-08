use starbase_styles::theme::create_graphical_theme;

pub fn set_miette_hooks() {
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
