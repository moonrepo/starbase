use starbase_args::*;

fn extract_commands(line: &CommandLine) -> &Vec<Sequence> {
    if let Some(Pipeline::Start(commands)) = line.0.first() {
        return &commands.0;
    }

    unimplemented!()
}

fn extract_args(line: &CommandLine) -> &Vec<Argument> {
    if let Some(Pipeline::Start(commands)) = line.0.first() {
        if let Some(Sequence::Start(command)) = commands.0.first() {
            return &command.0;
        }
    }

    unimplemented!()
}

macro_rules! test_commands {
    ($input:expr, $output:expr) => {
        let commands = parse($input).unwrap();
        assert_eq!(extract_commands(&commands), &$output);
        assert_eq!(commands.to_string(), $input);
    };
}

macro_rules! test_args {
    ($input:expr, $output:expr) => {
        let args = parse($input).unwrap();
        assert_eq!(extract_args(&args), &$output);
        assert_eq!(args.to_string(), $input);
    };
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

mod command_list {
    use super::*;

    #[test]
    fn redirects() {
        for op in [
            ">", ">>", ">>>", "<", "<<", "<<<", "<>", "&>", "&>>", ">&", "<&", ">|",
        ] {
            // test_commands!(
            //     format!("foo {op} bar"),
            //     [
            //         Sequence::Start(Command(vec![Argument::Value(Value::Unquoted(
            //             "foo".into()
            //         ))])),
            //         Sequence::Redirect(
            //             Command(vec![Argument::Value(Value::Unquoted("bar".into()))]),
            //             op.into()
            //         ),
            //     ]
            // );

            test_commands!(
                format!("foo 1{op} bar"),
                [
                    Sequence::Start(Command(vec![Argument::Value(Value::Unquoted(
                        "foo".into()
                    ))])),
                    Sequence::Redirect(
                        Command(vec![Argument::Value(Value::Unquoted("bar".into()))]),
                        format!("1{op}")
                    ),
                ]
            );
        }
    }

    #[test]
    fn terminators() {
        test_commands!(
            "foo;",
            [
                Sequence::Start(Command(vec![Argument::Value(Value::Unquoted(
                    "foo".into()
                ))])),
                Sequence::Stop(";".into())
            ]
        );
        test_commands!(
            "foo &",
            [
                Sequence::Start(Command(vec![Argument::Value(Value::Unquoted(
                    "foo".into()
                ))])),
                Sequence::Stop("&".into())
            ]
        );
        test_commands!(
            "foo 2>&1",
            [
                Sequence::Start(Command(vec![Argument::Value(Value::Unquoted(
                    "foo".into()
                ))])),
                Sequence::Stop("2>&1".into())
            ]
        );
    }

    #[test]
    fn terminators_spacing() {
        assert_eq!(parse(" foo;  ").unwrap().to_string(), "foo;");
        assert_eq!(parse(" foo ;").unwrap().to_string(), "foo;");
        assert_eq!(parse("foo  ; ").unwrap().to_string(), "foo;");

        assert_eq!(parse(" foo&").unwrap().to_string(), "foo &");
        assert_eq!(parse("foo &").unwrap().to_string(), "foo &");
        assert_eq!(parse(" foo  &  ").unwrap().to_string(), "foo &");
    }

    #[test]
    fn then() {
        test_commands!(
            "foo; bar -a; baz --qux",
            [
                Sequence::Start(Command(vec![Argument::Value(Value::Unquoted(
                    "foo".into()
                ))])),
                Sequence::Then(Command(vec![
                    Argument::Value(Value::Unquoted("bar".into())),
                    Argument::Flag("-a".into())
                ])),
                Sequence::Then(Command(vec![
                    Argument::Value(Value::Unquoted("baz".into())),
                    Argument::Option("--qux".into(), None)
                ])),
            ]
        );
        test_commands!(
            "foo; bar;",
            [
                Sequence::Start(Command(vec![Argument::Value(Value::Unquoted(
                    "foo".into()
                ))])),
                Sequence::Then(Command(vec![Argument::Value(Value::Unquoted(
                    "bar".into()
                )),])),
                Sequence::Stop(";".into())
            ]
        );
    }

    #[test]
    fn then_spacing() {
        assert_eq!(parse("foo;bar").unwrap().to_string(), "foo; bar");
        assert_eq!(parse("foo ; bar").unwrap().to_string(), "foo; bar");
        assert_eq!(
            parse("foo;   bar   ;baz").unwrap().to_string(),
            "foo; bar; baz"
        );
    }
}

mod command {
    use super::*;

    #[test]
    fn simple() {
        let mut command = String::from("bin");
        let mut actual = vec![Argument::Value(Value::Unquoted("bin".into()))];

        test_args!(command.as_str(), actual);

        command.push_str(" -a");
        actual.push(Argument::Flag("-a".into()));

        test_args!(command.as_str(), actual);

        command.push_str(" -xYZ");
        actual.push(Argument::FlagGroup("-xYZ".into()));

        test_args!(command.as_str(), actual);

        command.push_str(" --opt1=value");
        actual.push(Argument::Option(
            "--opt1".into(),
            Some(Value::Unquoted("value".into())),
        ));

        test_args!(command.as_str(), actual);

        command.push_str(" --opt-2='some value'");
        actual.push(Argument::Option(
            "--opt-2".into(),
            Some(Value::SingleQuoted("some value".into())),
        ));

        test_args!(command.as_str(), actual);

        command.push_str(" --opt_3=$'another value'");
        actual.push(Argument::Option(
            "--opt_3".into(),
            Some(Value::AnsiQuoted("another value".into())),
        ));

        test_args!(command.as_str(), actual);

        command.push_str(" --opt.4 \"last value\"");
        actual.push(Argument::Option("--opt.4".into(), None));
        actual.push(Argument::Value(Value::DoubleQuoted("last value".into())));

        test_args!(command.as_str(), actual);
    }

    #[test]
    fn spacing() {
        assert_eq!(
            extract_args(&parse(" a    b  c       -d ").unwrap()),
            &[
                Argument::Value(Value::Unquoted("a".into())),
                Argument::Value(Value::Unquoted("b".into())),
                Argument::Value(Value::Unquoted("c".into())),
                Argument::Flag("-d".into()),
            ]
        );
    }
}

mod args {
    use super::*;

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
