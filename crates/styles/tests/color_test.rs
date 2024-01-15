use starbase_styles::color::apply_style_tags;
use std::env;

#[test]
fn replaces_tags() {
    env::set_var("FORCE_COLOR", "1");
    env::remove_var("NO_COLOR");

    assert_eq!(apply_style_tags("this <file>is</file> a <caution>string <property>with</property></caution> many <success>style</success> tags!"), "this \u{1b}[38;5;36mis\u{1b}[0m a \u{1b}[38;5;208mstring \u{1b}[38;5;147mwith\u{1b}[0m\u{1b}[0m many \u{1b}[38;5;41mstyle\u{1b}[0m tags!");
}

#[test]
fn ignores_unknown_tags() {
    env::set_var("FORCE_COLOR", "1");
    env::remove_var("NO_COLOR");

    assert_eq!(
        apply_style_tags("this is <unknown>unknown</unknown>"),
        "this is <unknown>unknown</unknown>"
    );
}
