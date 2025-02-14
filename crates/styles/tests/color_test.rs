use starbase_styles::color::{apply_style_tags, parse_style_tags};
use starbase_styles::Style;
use std::env;

#[test]
fn replaces_tags() {
    env::set_var("FORCE_COLOR", "1");
    env::remove_var("NO_COLOR");

    assert_eq!(apply_style_tags("this <file>is</file> a <caution>string <property>with</property></caution> many <success>style</success> tags!"), "this \u{1b}[38;5;36mis\u{1b}[0m a \u{1b}[38;5;208mstring \u{1b}[0m\u{1b}[38;5;147mwith\u{1b}[0m many \u{1b}[38;5;41mstyle\u{1b}[0m tags!");
}

mod parse_tags {
    use super::*;

    #[test]
    fn no_tags() {
        assert_eq!(
            parse_style_tags("this has no tags"),
            vec![("this has no tags".to_owned(), None)]
        );
    }

    #[test]
    fn only_tag() {
        assert_eq!(
            parse_style_tags("<id>id</id>"),
            vec![("id".to_owned(), Some(Style::Id))]
        );
    }

    #[test]
    fn with_one_tag() {
        assert_eq!(
            parse_style_tags("this has one <id>tag</id>!"),
            vec![
                ("this has one ".to_owned(), None),
                ("tag".to_owned(), Some(Style::Id)),
                ("!".to_owned(), None),
            ]
        );
    }

    #[test]
    fn with_many_tag() {
        assert_eq!(
            parse_style_tags("this has <property>more</property> than one <path>tag</path>!"),
            vec![
                ("this has ".to_owned(), None),
                ("more".to_owned(), Some(Style::Property)),
                (" than one ".to_owned(), None),
                ("tag".to_owned(), Some(Style::Path)),
                ("!".to_owned(), None),
            ]
        );
    }

    #[test]
    fn with_nested_tags() {
        assert_eq!(
            parse_style_tags("this <muted>is <mutedlight>two</mutedlight> colors</muted>"),
            vec![
                ("this ".to_owned(), None),
                ("is ".to_owned(), Some(Style::Muted)),
                ("two".to_owned(), Some(Style::MutedLight)),
                (" colors".to_owned(), Some(Style::Muted)),
            ]
        );
    }

    #[test]
    fn tag_at_start() {
        assert_eq!(
            parse_style_tags("<id>tag</id> suffix"),
            vec![
                ("tag".to_owned(), Some(Style::Id)),
                (" suffix".to_owned(), None),
            ]
        );
    }

    #[test]
    fn tag_at_end() {
        assert_eq!(
            parse_style_tags("prefix <id>tag</id>"),
            vec![
                ("prefix ".to_owned(), None),
                ("tag".to_owned(), Some(Style::Id)),
            ]
        );
    }

    #[test]
    fn no_whitespace_around() {
        assert_eq!(
            parse_style_tags("prefix<id>tag</id>suffix"),
            vec![
                ("prefix".to_owned(), None),
                ("tag".to_owned(), Some(Style::Id)),
                ("suffix".to_owned(), None),
            ]
        );
    }

    #[test]
    fn has_whitespace_inside() {
        assert_eq!(
            parse_style_tags("prefix  <id> t a g </id>  suffix"),
            vec![
                ("prefix  ".to_owned(), None),
                (" t a g ".to_owned(), Some(Style::Id)),
                ("  suffix".to_owned(), None),
            ]
        );
    }

    #[test]
    fn ignores_lt_char() {
        assert_eq!(
            parse_style_tags("this is < 3"),
            vec![("this is < 3".to_owned(), None)]
        );
    }

    #[test]
    fn ignores_gt_char() {
        assert_eq!(
            parse_style_tags("this is > 3"),
            vec![("this is > 3".to_owned(), None)]
        );
    }

    #[test]
    fn ignores_gt_and_lt_not_being_a_tag() {
        assert_eq!(
            parse_style_tags("this is > 3 and < 5"),
            vec![("this is > 3 and < 5".to_owned(), None)]
        );
    }

    #[test]
    fn ignores_lt_and_gt_not_being_a_tag() {
        assert_eq!(
            parse_style_tags("this is < 3 and > 5"),
            vec![("this is < 3 and > 5".to_owned(), None)]
        );
    }

    #[test]
    fn ignores_unknown_tag() {
        assert_eq!(
            parse_style_tags("<unknown>tag</unknown>"),
            vec![("tag".to_owned(), None)]
        );
    }

    #[test]
    #[should_panic(expected = "Close tag `file` found without an open tag")]
    fn errors_no_open_tag() {
        parse_style_tags("tag</file>");
    }
}
