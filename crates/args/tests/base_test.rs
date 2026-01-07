use starbase_args::{Argument, Value, parse, parse_args};

#[test]
fn syntax() {
    parse_args(r#""""#);
    parse_args(r#""quote""#);
    parse_args(r#""quo te""#);
    parse_args(r#""quo"te""#);

    assert!(false);
}

mod pipeline {
    use super::*;

    #[test]
    fn only_command() {
        assert_eq!(
            parse_args("foo --bar"),
            [
                Argument::EnvVar("KEY".into(), Value::Unquoted("value".into()), None),
                Argument::Value(Value::Unquoted("bin".into())),
            ]
        );
    }

    #[test]
    fn multi_command_then() {
        assert_eq!(parse("foo --bar; baz -x;"), []);
    }

    #[test]
    fn pipe() {
        assert_eq!(
            parse_args("foo -a | bar --b | baz 'c'"),
            [
                Argument::EnvVar("KEY".into(), Value::Unquoted("value".into()), None),
                Argument::Value(Value::Unquoted("bin".into())),
            ]
        );
    }

    #[test]
    fn pipe_error() {
        assert_eq!(
            parse_args("foo -a |& bar --b |& baz 'c'"),
            [
                Argument::EnvVar("KEY".into(), Value::Unquoted("value".into()), None),
                Argument::Value(Value::Unquoted("bin".into())),
            ]
        );
    }
}

mod args_base {
    use super::*;

    #[test]
    fn env_var() {
        assert_eq!(
            parse_args("KEY=value bin"),
            [
                Argument::EnvVar("KEY".into(), Value::Unquoted("value".into()), None),
                Argument::Value(Value::Unquoted("bin".into())),
            ]
        );
        assert_eq!(
            parse_args("KEY='a b c' bin"),
            [
                Argument::EnvVar("KEY".into(), Value::SingleQuoted("a b c".into()), None),
                Argument::Value(Value::Unquoted("bin".into())),
            ]
        );
        assert_eq!(
            parse_args("KEY=\"a b c\" bin"),
            [
                Argument::EnvVar("KEY".into(), Value::DoubleQuoted("a b c".into()), None),
                Argument::Value(Value::Unquoted("bin".into())),
            ]
        );
        assert_eq!(
            parse_args("KEY1=1 $env:KEY2=2 $ENV.KEY3=3 bin"),
            [
                Argument::EnvVar("KEY1".into(), Value::Unquoted("1".into()), None),
                Argument::EnvVar(
                    "KEY2".into(),
                    Value::Unquoted("2".into()),
                    Some("$env:".into())
                ),
                Argument::EnvVar(
                    "KEY3".into(),
                    Value::Unquoted("3".into()),
                    Some("$ENV.".into())
                ),
                Argument::Value(Value::Unquoted("bin".into())),
            ]
        );
    }

    #[test]
    fn env_var_with_namespace() {
        for prefix in ["$e:", "$env:", "$env."] {
            assert_eq!(
                parse_args(format!("{prefix}KEY=value")),
                [Argument::EnvVar(
                    "KEY".into(),
                    Value::Unquoted("value".into()),
                    Some(prefix.into())
                )]
            );
            assert_eq!(
                parse_args(format!("{}KEY='value'", prefix.to_uppercase())),
                [Argument::EnvVar(
                    "KEY".into(),
                    Value::SingleQuoted("value".into()),
                    Some(prefix.to_uppercase())
                )]
            );
        }
    }

    #[test]
    fn special_quote() {
        assert_eq!(
            parse_args("bin $''"),
            [
                Argument::Value(Value::Unquoted("bin".into())),
                Argument::Value(Value::AnsiQuoted("".into()))
            ]
        );
        assert_eq!(
            parse_args("bin $'abc'"),
            [
                Argument::Value(Value::Unquoted("bin".into())),
                Argument::Value(Value::AnsiQuoted("abc".into()))
            ]
        );
        assert_eq!(
            parse_args("bin $'a b c'"),
            [
                Argument::Value(Value::Unquoted("bin".into())),
                Argument::Value(Value::AnsiQuoted("a b c".into()))
            ]
        );
    }

    #[test]
    fn single_quote() {
        assert_eq!(
            parse_args("bin ''"),
            [
                Argument::Value(Value::Unquoted("bin".into())),
                Argument::Value(Value::SingleQuoted("".into()))
            ]
        );
        assert_eq!(
            parse_args("bin 'abc'"),
            [
                Argument::Value(Value::Unquoted("bin".into())),
                Argument::Value(Value::SingleQuoted("abc".into()))
            ]
        );
        assert_eq!(
            parse_args("bin 'a b c'"),
            [
                Argument::Value(Value::Unquoted("bin".into())),
                Argument::Value(Value::SingleQuoted("a b c".into()))
            ]
        );
    }

    #[test]
    fn double_quote() {
        assert_eq!(
            parse_args("bin \"\""),
            [
                Argument::Value(Value::Unquoted("bin".into())),
                Argument::Value(Value::DoubleQuoted("".into()))
            ]
        );
        assert_eq!(
            parse_args("bin \"abc\""),
            [
                Argument::Value(Value::Unquoted("bin".into())),
                Argument::Value(Value::DoubleQuoted("abc".into()))
            ]
        );
        assert_eq!(
            parse_args("bin \"a b c\""),
            [
                Argument::Value(Value::Unquoted("bin".into())),
                Argument::Value(Value::DoubleQuoted("a b c".into()))
            ]
        );
    }

    #[test]
    fn exe() {
        assert_eq!(
            parse_args("bin"),
            [Argument::Value(Value::Unquoted("bin".into()))]
        );
        assert_eq!(
            parse_args("file.sh"),
            [Argument::Value(Value::Unquoted("file.sh".into()))]
        );
        assert_eq!(
            parse_args("./file.sh"),
            [Argument::Value(Value::Unquoted("./file.sh".into()))]
        );
        assert_eq!(
            parse_args("../file.sh"),
            [Argument::Value(Value::Unquoted("../file.sh".into()))]
        );
        assert_eq!(
            parse_args("\"file with space.sh\""),
            [Argument::Value(Value::DoubleQuoted(
                "file with space.sh".into()
            ))]
        );
    }

    #[test]
    fn flags() {
        assert_eq!(
            parse_args("-aB -ABC -abcd"),
            [
                Argument::FlagGroup("-aB".into()),
                Argument::FlagGroup("-ABC".into()),
                Argument::FlagGroup("-abcd".into())
            ]
        );
        assert_eq!(
            parse_args("-a -B -c -D"),
            [
                Argument::Flag("-a".into()),
                Argument::Flag("-B".into()),
                Argument::Flag("-c".into()),
                Argument::Flag("-D".into()),
            ]
        );
    }
}
