use starbase_args::*;

fn extract_args(line: &CommandLine) -> &Vec<Argument> {
    if let Some(Pipeline::Start(commands)) = line.0.first() {
        if let Some(Sequence::Start(command)) = commands.0.first() {
            return &command.0;
        }
    }

    unimplemented!()
}

#[test]
fn syntax() {
    dbg!(parse("command1  ||    command2  |command3").unwrap());

    assert!(false);
}

mod pipeline {
    use super::*;

    #[test]
    fn single_command() {
        assert_eq!(
            parse("foo --bar").unwrap(),
            CommandLine(vec![Pipeline::Start(CommandList(vec![Sequence::Start(
                Command(vec![
                    Argument::Value(Value::Unquoted("foo".into())),
                    Argument::Option("--bar".into(), None),
                ])
            )]))])
        );
    }

    // #[test]
    // fn multi_command_then() {
    //     assert_eq!(parse("foo --bar; baz -x;"), []);
    // }

    // #[test]
    // fn pipe() {
    //     assert_eq!(
    //         parse_args("foo -a | bar --b | baz 'c'"),
    //         [
    //             Argument::EnvVar("KEY".into(), Value::Unquoted("value".into()), None),
    //             Argument::Value(Value::Unquoted("bin".into())),
    //         ]
    //     );
    // }

    // #[test]
    // fn pipe_error() {
    //     assert_eq!(
    //         parse_args("foo -a |& bar --b |& baz 'c'"),
    //         [
    //             Argument::EnvVar("KEY".into(), Value::Unquoted("value".into()), None),
    //             Argument::Value(Value::Unquoted("bin".into())),
    //         ]
    //     );
    // }
}

mod args {
    use super::*;

    macro_rules! test_args {
        ($input:expr, $output:expr) => {
            let args = parse($input).unwrap();
            assert_eq!(extract_args(&args), &$output);
            assert_eq!(args.to_string(), $input);
        };
    }

    #[test]
    fn env_var() {
        test_args!(
            "KEY=value bin",
            [
                Argument::EnvVar("KEY".into(), Value::Unquoted("value".into()), None),
                Argument::Value(Value::Unquoted("bin".into())),
            ]
        );
        test_args!(
            "KEY='a b c' bin",
            [
                Argument::EnvVar("KEY".into(), Value::SingleQuoted("a b c".into()), None),
                Argument::Value(Value::Unquoted("bin".into())),
            ]
        );
        test_args!(
            "KEY=\"a b c\" bin",
            [
                Argument::EnvVar("KEY".into(), Value::DoubleQuoted("a b c".into()), None),
                Argument::Value(Value::Unquoted("bin".into())),
            ]
        );
        test_args!(
            "KEY1=1 $env:KEY2=2 $ENV.KEY3=3 bin",
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
            test_args!(
                format!("{prefix}KEY=value"),
                [Argument::EnvVar(
                    "KEY".into(),
                    Value::Unquoted("value".into()),
                    Some(prefix.into())
                )]
            );
            test_args!(
                format!("{}KEY='value'", prefix.to_uppercase()),
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
        test_args!(
            "bin $''",
            [
                Argument::Value(Value::Unquoted("bin".into())),
                Argument::Value(Value::AnsiQuoted("".into()))
            ]
        );
        test_args!(
            "bin $'abc'",
            [
                Argument::Value(Value::Unquoted("bin".into())),
                Argument::Value(Value::AnsiQuoted("abc".into()))
            ]
        );
        test_args!(
            "bin $'a b c'",
            [
                Argument::Value(Value::Unquoted("bin".into())),
                Argument::Value(Value::AnsiQuoted("a b c".into()))
            ]
        );
    }

    #[test]
    fn single_quote() {
        test_args!(
            "bin ''",
            [
                Argument::Value(Value::Unquoted("bin".into())),
                Argument::Value(Value::SingleQuoted("".into()))
            ]
        );
        test_args!(
            "bin 'abc'",
            [
                Argument::Value(Value::Unquoted("bin".into())),
                Argument::Value(Value::SingleQuoted("abc".into()))
            ]
        );
        test_args!(
            "bin 'a b c'",
            [
                Argument::Value(Value::Unquoted("bin".into())),
                Argument::Value(Value::SingleQuoted("a b c".into()))
            ]
        );
        test_args!(
            "bin 'a\\'b'",
            [
                Argument::Value(Value::Unquoted("bin".into())),
                Argument::Value(Value::SingleQuoted("a\\'b".into()))
            ]
        );
    }

    #[test]
    fn double_quote() {
        test_args!(
            "bin \"\"",
            [
                Argument::Value(Value::Unquoted("bin".into())),
                Argument::Value(Value::DoubleQuoted("".into()))
            ]
        );
        test_args!(
            "bin \"abc\"",
            [
                Argument::Value(Value::Unquoted("bin".into())),
                Argument::Value(Value::DoubleQuoted("abc".into()))
            ]
        );
        test_args!(
            "bin \"a b c\"",
            [
                Argument::Value(Value::Unquoted("bin".into())),
                Argument::Value(Value::DoubleQuoted("a b c".into()))
            ]
        );
        test_args!(
            "bin \"a\\\"b\"",
            [
                Argument::Value(Value::Unquoted("bin".into())),
                Argument::Value(Value::DoubleQuoted("a\\\"b".into()))
            ]
        );
    }

    #[test]
    fn exe() {
        test_args!("bin", [Argument::Value(Value::Unquoted("bin".into()))]);
        test_args!(
            "file.sh",
            [Argument::Value(Value::Unquoted("file.sh".into()))]
        );
        test_args!(
            "./file.sh",
            [Argument::Value(Value::Unquoted("./file.sh".into()))]
        );
        test_args!(
            "../file.sh",
            [Argument::Value(Value::Unquoted("../file.sh".into()))]
        );
        test_args!(
            "\"file with space.sh\"",
            [Argument::Value(Value::DoubleQuoted(
                "file with space.sh".into()
            ))]
        );
    }

    #[test]
    fn flags() {
        test_args!(
            "-aB -ABC -abcd",
            [
                Argument::FlagGroup("-aB".into()),
                Argument::FlagGroup("-ABC".into()),
                Argument::FlagGroup("-abcd".into())
            ]
        );
        test_args!(
            "-a -B -c -D",
            [
                Argument::Flag("-a".into()),
                Argument::Flag("-B".into()),
                Argument::Flag("-c".into()),
                Argument::Flag("-D".into()),
            ]
        );
    }

    #[test]
    fn options() {
        test_args!(
            "--a --B --c.d --e-f --g_h --iJ",
            [
                Argument::Option("--a".into(), None),
                Argument::Option("--B".into(), None),
                Argument::Option("--c.d".into(), None),
                Argument::Option("--e-f".into(), None),
                Argument::Option("--g_h".into(), None),
                Argument::Option("--iJ".into(), None)
            ]
        );
        test_args!(
            "--a=a --b='b b' --c=\"c c c\" --d=$'d'",
            [
                Argument::Option("--a".into(), Some(Value::Unquoted("a".into()))),
                Argument::Option("--b".into(), Some(Value::SingleQuoted("b b".into()))),
                Argument::Option("--c".into(), Some(Value::DoubleQuoted("c c c".into()))),
                Argument::Option("--d".into(), Some(Value::AnsiQuoted("d".into())))
            ]
        );
    }
}
