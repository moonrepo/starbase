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

macro_rules! test_pipeline {
    ($input:expr, $output:expr) => {
        let pipeline = parse($input).unwrap();
        assert_eq!(&pipeline, &$output);
        assert_eq!(pipeline.to_string(), $input);
    };
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

mod examples {
    use super::*;

    #[test]
    fn awk() {
        let actual = CommandLine(vec![Pipeline::Start(CommandList(vec![Sequence::Start(
            Command(vec![
                Argument::Value(Value::Unquoted("awk".into())),
                Argument::Value(Value::SingleQuoted("NR == 1 {min = max = $2} $2 < min {min = $2} $2 > max {max = $2} {sum += $2} END {printf(\"Min: %.2f, Max: %.2f, Avg: %.2f\\n\", min, max, sum/NR)}".into())),
                Argument::Value(Value::Unquoted("filename.txt".into())),
            ]),
        )]))]);

        assert_eq!(
            parse("awk 'NR == 1 {min = max = $2} $2 < min {min = $2} $2 > max {max = $2} {sum += $2} END {printf(\"Min: %.2f, Max: %.2f, Avg: %.2f\\n\", min, max, sum/NR)}' filename.txt").unwrap(),
            actual
        );
    }

    #[test]
    fn bash() {
        let actual = CommandLine(vec![Pipeline::Start(CommandList(vec![Sequence::Start(
            Command(vec![Argument::Value(Value::Substitution(
                Substitution::Command("$( echo ${FOO} && echo hi )".into()),
            ))]),
        )]))]);

        assert_eq!(parse("$( echo ${FOO} && echo hi )").unwrap(), actual);
    }

    #[test]
    fn curl() {
        let actual = CommandLine(vec![
            Pipeline::Start(CommandList(vec![Sequence::Start(Command(vec![
                Argument::Value(Value::Unquoted("curl".into())),
                Argument::Flag("-s".into()),
                Argument::Flag("-L".into()),
                Argument::Value(Value::Expansion(Expansion::Param("$uri".into()))),
            ]))])),
            Pipeline::Pipe(CommandList(vec![Sequence::Start(Command(vec![
                Argument::Value(Value::Unquoted("tar".into())),
                Argument::FlagGroup("-xzvf".into()),
                Argument::Value(Value::Unquoted("-".into())),
                Argument::Flag("-C".into()),
                Argument::Value(Value::Unquoted(".".into())),
            ]))])),
        ]);

        assert_eq!(parse("curl -s -L $uri | tar -xzvf - -C .").unwrap(), actual);
    }

    #[test]
    fn git() {
        let actual = CommandLine(vec![Pipeline::Start(CommandList(vec![Sequence::Start(
            Command(vec![
                Argument::Value(Value::Unquoted("git".into())),
                Argument::Value(Value::Unquoted("rebase".into())),
                Argument::Flag("-i".into()),
                Argument::Option("--empty".into(), Some(Value::Unquoted("drop".into()))),
                Argument::Option("--exec".into(), None),
                Argument::Value(Value::DoubleQuoted("echo".into())),
                Argument::Value(Value::Unquoted("HEAD~3".into())),
            ]),
        )]))]);

        assert_eq!(
            parse("git rebase -i --empty=drop --exec \"echo\" HEAD~3").unwrap(),
            actual
        );

        let actual = CommandLine(vec![Pipeline::Start(CommandList(vec![Sequence::Start(
            Command(vec![
                Argument::Value(Value::Unquoted("git".into())),
                Argument::Value(Value::Unquoted("checkout".into())),
                Argument::Flag("-b".into()),
                Argument::Value(Value::DoubleQuoted("🚀-emoji".into())),
            ]),
        )]))]);

        assert_eq!(parse("git checkout -b \"🚀-emoji\"").unwrap(), actual);

        let actual = CommandLine(vec![Pipeline::Start(CommandList(vec![Sequence::Start(
            Command(vec![
                Argument::Value(Value::Unquoted("git".into())),
                Argument::Value(Value::Unquoted("reset".into())),
                Argument::Option("--hard".into(), None),
                Argument::Value(Value::Expansion(Expansion::Brace("HEAD@{2}".into()))),
            ]),
        )]))]);

        assert_eq!(parse("git reset --hard HEAD@{2}").unwrap(), actual);
    }

    #[test]
    fn docker() {
        let actual = CommandLine(vec![Pipeline::Start(CommandList(vec![
            Sequence::Start(Command(vec![
                Argument::Value(Value::Unquoted("docker".into())),
                Argument::Value(Value::Unquoted("run".into())),
                Argument::FlagGroup("-it".into()),
                Argument::Option("--name".into(), Some(Value::DoubleQuoted("foo".into()))),
                Argument::Flag("-v".into()),
                Argument::Value(Value::Unquoted("/foo/bar:/fizz/buzz:to".into())),
                Argument::Value(Value::Unquoted("lol.domain/blah".into())),
                Argument::Value(Value::Unquoted("blah:pass@sha256__abe8euenr93nd".into())),
            ])),
            Sequence::Passthrough(Command(vec![
                Argument::Value(Value::Unquoted("bash".into())),
                Argument::Flag("-c".into()),
                Argument::Value(Value::SingleQuoted("do something".into())),
            ])),
        ]))]);

        assert_eq!(
            parse(
                "docker run -it --name=\"foo\" -v /foo/bar:/fizz/buzz:to lol.domain/blah blah:pass@sha256__abe8euenr93nd -- bash -c 'do something'",
            ).unwrap(),
            actual
        );

        let actual = CommandLine(vec![Pipeline::Start(CommandList(vec![Sequence::Start(
            Command(vec![
                Argument::Value(Value::Unquoted("docker".into())),
                Argument::Value(Value::Unquoted("build".into())),
                Argument::Option("--target".into(), None),
                Argument::Value(Value::Unquoted("prod".into())),
                Argument::Flag("-t".into()),
                Argument::Value(Value::Expansion(Expansion::Param("$project".into()))),
                Argument::Flag("-f".into()),
                Argument::Value(Value::Unquoted("Dockerfile".into())),
                Argument::Value(Value::Expansion(Expansion::Param("$workspaceRoot".into()))),
                Argument::Option("--build-arg".into(), None),
                Argument::EnvVar(
                    "COMMIT_HASH".into(),
                    Value::Substitution(Substitution::Command("$(git rev-parse HEAD)".into())),
                    None,
                ),
            ]),
        )]))]);

        assert_eq!(
            parse("docker build --target prod -t $project -f Dockerfile $workspaceRoot --build-arg COMMIT_HASH=$(git rev-parse HEAD)").unwrap(),
            actual
        );
    }

    #[test]
    fn qemu() {
        let actual = CommandLine(vec![Pipeline::Start(CommandList(vec![Sequence::Start(
            Command(vec![
                Argument::Value(Value::Unquoted("qemu-system-x86_64".into())),
                Argument::FlagGroup("-machine".into()),
                Argument::Value(Value::Unquoted("q35,smm=on".into())),
                Argument::FlagGroup("-drive".into()),
                Argument::Value(Value::Unquoted(
                    "if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE.secboot.fd"
                        .into(),
                )),
                Argument::FlagGroup("-global".into()),
                Argument::Value(Value::Unquoted(
                    "driver=cfi.pflash01,property=secure,value=on".into(),
                )),
                Argument::FlagGroup("-drive".into()),
                Argument::Value(Value::Unquoted(
                    "file=rbd:pool/volume:id=admin:key=AQAAABCDEF==:conf=/etc/ceph/ceph.conf,format=raw,if=virtio,id=drive1,cache=none"
                        .into(),
                )),
                Argument::FlagGroup("-device".into()),
                Argument::Value(Value::Unquoted("usb-tablet".into())),
                Argument::FlagGroup("-vnc".into()),
                Argument::Value(Value::Unquoted("127.0.0.1:0".into())),
                Argument::FlagGroup("-device".into()),
                Argument::Value(Value::Unquoted("vfio-pci,host=0000:01:00.0,multifunction=on".into())),
                Argument::FlagGroup("-netdev".into()),
                Argument::Value(Value::Unquoted("user,id=net0,hostfwd=tcp::2222-:22".into())),
                Argument::FlagGroup("-device".into()),
                Argument::Value(Value::Unquoted("e1000e,netdev=net0".into())),
                Argument::FlagGroup("-qmp".into()),
                Argument::Value(Value::Unquoted("unix:/tmp/qmp.sock,server=on,wait=off".into())),
            ]),
        )]))]);

        assert_eq!(parse("qemu-system-x86_64 -machine q35,smm=on -drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE.secboot.fd -global driver=cfi.pflash01,property=secure,value=on -drive file=rbd:pool/volume:id=admin:key=AQAAABCDEF==:conf=/etc/ceph/ceph.conf,format=raw,if=virtio,id=drive1,cache=none -device usb-tablet -vnc 127.0.0.1:0 -device vfio-pci,host=0000:01:00.0,multifunction=on -netdev user,id=net0,hostfwd=tcp::2222-:22 -device e1000e,netdev=net0 -qmp unix:/tmp/qmp.sock,server=on,wait=off").unwrap(), actual);
    }

    #[test]
    fn system() {
        let actual = CommandLine(vec![Pipeline::Start(CommandList(vec![Sequence::Start(
            Command(vec![
                Argument::Value(Value::Unquoted("ls".into())),
                Argument::Flag("-l".into()),
                Argument::Value(Value::SingleQuoted("afile; rm -rf ~".into())),
            ]),
        )]))]);

        assert_eq!(parse("ls -l 'afile; rm -rf ~'").unwrap(), actual);

        let actual = CommandLine(vec![Pipeline::Start(CommandList(vec![
            Sequence::Start(Command(vec![
                Argument::Value(Value::Unquoted("apt-get".into())),
                Argument::Value(Value::Unquoted("update".into())),
                Argument::FlagGroup("-qq".into()),
            ])),
            Sequence::Redirect(
                Command(vec![Argument::Value(Value::Unquoted("/dev/null".into()))]),
                ">".into(),
            ),
            Sequence::AndThen(Command(vec![
                Argument::Value(Value::Unquoted("apt-get".into())),
                Argument::Value(Value::Unquoted("install".into())),
                Argument::FlagGroup("-yq".into()),
                Argument::Value(Value::Unquoted("jq".into())),
                Argument::Value(Value::Unquoted("make".into())),
            ])),
            Sequence::Redirect(
                Command(vec![Argument::Value(Value::Unquoted("/dev/null".into()))]),
                ">>".into(),
            ),
            Sequence::Stop("2>&1".into()),
        ]))]);

        assert_eq!(
            parse("apt-get update -qq >/dev/null && apt-get install -yq jq make >>/dev/null 2>&1")
                .unwrap(),
            actual
        );
    }
}

mod pipeline {
    use super::*;

    #[test]
    fn simple_command() {
        let actual = CommandLine(vec![Pipeline::Start(CommandList(vec![Sequence::Start(
            Command(vec![
                Argument::Value(Value::Unquoted("foo".into())),
                Argument::Option("--bar".into(), None),
            ]),
        )]))]);

        assert_eq!(parse("foo --bar").unwrap(), actual);
    }

    #[test]
    fn complex_commands() {
        let actual = CommandLine(vec![
            Pipeline::Start(CommandList(vec![
                Sequence::Start(Command(vec![
                    Argument::Value(Value::Unquoted("foo".into())),
                    Argument::Option("--a".into(), None),
                ])),
                Sequence::Redirect(
                    Command(vec![Argument::Value(Value::Unquoted("out.txt".into()))]),
                    ">>".into(),
                ),
                Sequence::AndThen(Command(vec![
                    Argument::Value(Value::Unquoted("bar".into())),
                    Argument::FlagGroup("-bC".into()),
                    Argument::Value(Value::SingleQuoted("value".into())),
                ])),
                Sequence::OrElse(Command(vec![
                    Argument::Value(Value::Unquoted("exit".into())),
                    Argument::Value(Value::Expansion(Expansion::Param("$ret".into()))),
                ])),
            ])),
            Pipeline::Pipe(CommandList(vec![
                Sequence::Start(Command(vec![
                    Argument::Value(Value::Unquoted("baz".into())),
                    Argument::Value(Value::Substitution(Substitution::Process("<(in)".into()))),
                ])),
                Sequence::OrElse(Command(vec![
                    Argument::Value(Value::Unquoted("baz".into())),
                    Argument::Value(Value::Substitution(Substitution::Command(
                        "$(./out.sh)".into(),
                    ))),
                ])),
                Sequence::AndThen(Command(vec![
                    Argument::Value(Value::Unquoted("qux".into())),
                    Argument::Value(Value::Unquoted("1".into())),
                    Argument::Value(Value::Unquoted("2".into())),
                    Argument::Value(Value::Unquoted("3".into())),
                ])),
                Sequence::Passthrough(Command(vec![Argument::Value(Value::Unquoted(
                    "wat".into(),
                ))])),
            ])),
            Pipeline::PipeAll(CommandList(vec![
                Sequence::Start(Command(vec![Argument::Value(Value::Unquoted(
                    "last".into(),
                ))])),
                Sequence::Stop("&".into()),
            ])),
        ]);

        assert_eq!(parse("foo --a >> out.txt && bar -bC 'value' || exit $ret | baz <(in) || baz $(./out.sh) && qux 1 2 3 -- wat |& last &").unwrap(), actual);
    }

    #[test]
    fn pipe() {
        let actual = CommandLine(vec![
            Pipeline::Start(CommandList(vec![Sequence::Start(Command(vec![
                Argument::Value(Value::Unquoted("foo".into())),
                Argument::Flag("-a".into()),
            ]))])),
            Pipeline::Pipe(CommandList(vec![Sequence::Start(Command(vec![
                Argument::Value(Value::Unquoted("bar".into())),
                Argument::Option("--b".into(), None),
            ]))])),
            Pipeline::PipeAll(CommandList(vec![Sequence::Start(Command(vec![
                Argument::Value(Value::Unquoted("baz".into())),
                Argument::Value(Value::SingleQuoted("c".into())),
            ]))])),
        ]);

        assert_eq!(parse("foo -a | bar --b |& baz 'c'").unwrap(), actual);
        assert_eq!(
            parse("foo -a  |    bar --b     |&      baz 'c'").unwrap(),
            actual
        );
        assert_eq!(parse("foo -a|bar --b|&baz 'c'").unwrap(), actual);
    }

    #[test]
    fn pipe_negated() {
        let actual = CommandLine(vec![
            Pipeline::StartNegated(CommandList(vec![Sequence::Start(Command(vec![
                Argument::Value(Value::Unquoted("foo".into())),
                Argument::Flag("-a".into()),
            ]))])),
            Pipeline::Pipe(CommandList(vec![Sequence::Start(Command(vec![
                Argument::Value(Value::Unquoted("bar".into())),
                Argument::Option("--b".into(), None),
            ]))])),
            Pipeline::PipeAll(CommandList(vec![Sequence::Start(Command(vec![
                Argument::Value(Value::Unquoted("baz".into())),
                Argument::Value(Value::SingleQuoted("c".into())),
            ]))])),
        ]);

        assert_eq!(parse("! foo -a | bar --b |& baz 'c'").unwrap(), actual);
        assert_eq!(
            parse("!foo -a  |    bar --b     |&      baz 'c'").unwrap(),
            actual
        );
        assert_eq!(parse("! foo -a|bar --b|&baz 'c'").unwrap(), actual);
    }
}

mod command_list {
    use super::*;

    #[test]
    fn redirects() {
        for op in [
            ">", ">>", ">>>", "<", "<<", "<<<", "<>", "&>", "&>>", ">&", "<&", ">|", "<?", ">?",
            "<^", ">^",
        ] {
            test_commands!(
                format!("foo {op} bar"),
                [
                    Sequence::Start(Command(vec![Argument::Value(Value::Unquoted(
                        "foo".into()
                    ))])),
                    Sequence::Redirect(
                        Command(vec![Argument::Value(Value::Unquoted("bar".into()))]),
                        op.into()
                    ),
                ]
            );

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

            test_commands!(
                format!("foo {op}2 bar"),
                [
                    Sequence::Start(Command(vec![Argument::Value(Value::Unquoted(
                        "foo".into()
                    ))])),
                    Sequence::Redirect(
                        Command(vec![Argument::Value(Value::Unquoted("bar".into()))]),
                        format!("{op}2")
                    ),
                ]
            );

            test_commands!(
                format!("foo 1{op}2 bar"),
                [
                    Sequence::Start(Command(vec![Argument::Value(Value::Unquoted(
                        "foo".into()
                    ))])),
                    Sequence::Redirect(
                        Command(vec![Argument::Value(Value::Unquoted("bar".into()))]),
                        format!("1{op}2")
                    ),
                ]
            );

            test_commands!(
                format!("foo o{op}err bar"),
                [
                    Sequence::Start(Command(vec![Argument::Value(Value::Unquoted(
                        "foo".into()
                    ))])),
                    Sequence::Redirect(
                        Command(vec![Argument::Value(Value::Unquoted("bar".into()))]),
                        format!("o{op}err")
                    ),
                ]
            );

            test_commands!(
                format!("foo o+e{op} bar"),
                [
                    Sequence::Start(Command(vec![Argument::Value(Value::Unquoted(
                        "foo".into()
                    ))])),
                    Sequence::Redirect(
                        Command(vec![Argument::Value(Value::Unquoted("bar".into()))]),
                        format!("o+e{op}")
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

    #[test]
    fn and_then() {
        test_commands!(
            "foo && bar -a && baz --qux",
            [
                Sequence::Start(Command(vec![Argument::Value(Value::Unquoted(
                    "foo".into()
                ))])),
                Sequence::AndThen(Command(vec![
                    Argument::Value(Value::Unquoted("bar".into())),
                    Argument::Flag("-a".into())
                ])),
                Sequence::AndThen(Command(vec![
                    Argument::Value(Value::Unquoted("baz".into())),
                    Argument::Option("--qux".into(), None)
                ])),
            ]
        );
        test_commands!(
            "foo && bar &",
            [
                Sequence::Start(Command(vec![Argument::Value(Value::Unquoted(
                    "foo".into()
                ))])),
                Sequence::AndThen(Command(vec![Argument::Value(Value::Unquoted(
                    "bar".into()
                )),])),
                Sequence::Stop("&".into())
            ]
        );
    }

    #[test]
    fn and_then_spacing() {
        assert_eq!(parse("foo&&bar").unwrap().to_string(), "foo && bar");
        assert_eq!(parse("foo && bar").unwrap().to_string(), "foo && bar");
        assert_eq!(
            parse("foo&&   bar   &&baz").unwrap().to_string(),
            "foo && bar && baz"
        );
    }

    #[test]
    fn or_else() {
        test_commands!(
            "foo || bar -a || baz --qux",
            [
                Sequence::Start(Command(vec![Argument::Value(Value::Unquoted(
                    "foo".into()
                ))])),
                Sequence::OrElse(Command(vec![
                    Argument::Value(Value::Unquoted("bar".into())),
                    Argument::Flag("-a".into())
                ])),
                Sequence::OrElse(Command(vec![
                    Argument::Value(Value::Unquoted("baz".into())),
                    Argument::Option("--qux".into(), None)
                ])),
            ]
        );
        test_commands!(
            "foo || bar 2>&1",
            [
                Sequence::Start(Command(vec![Argument::Value(Value::Unquoted(
                    "foo".into()
                ))])),
                Sequence::OrElse(Command(vec![Argument::Value(Value::Unquoted(
                    "bar".into()
                )),])),
                Sequence::Stop("2>&1".into())
            ]
        );
    }

    #[test]
    fn or_else_spacing() {
        assert_eq!(parse("foo||bar").unwrap().to_string(), "foo || bar");
        assert_eq!(parse("foo || bar").unwrap().to_string(), "foo || bar");
        assert_eq!(
            parse("foo||   bar   ||baz").unwrap().to_string(),
            "foo || bar || baz"
        );
    }

    #[test]
    fn passthrough() {
        test_commands!(
            "foo -- bar --qux",
            [
                Sequence::Start(Command(vec![Argument::Value(Value::Unquoted(
                    "foo".into()
                ))])),
                Sequence::Passthrough(Command(vec![
                    Argument::Value(Value::Unquoted("bar".into())),
                    Argument::Option("--qux".into(), None)
                ])),
            ]
        );
    }

    #[test]
    fn command_substitution() {
        test_commands!(
            "echo $(foo)",
            [Sequence::Start(Command(vec![
                Argument::Value(Value::Unquoted("echo".into())),
                Argument::Value(Value::Substitution(Substitution::Command("$(foo)".into())))
            ]))]
        );
        test_commands!(
            "echo $(bar)",
            [Sequence::Start(Command(vec![
                Argument::Value(Value::Unquoted("echo".into())),
                Argument::Value(Value::Substitution(Substitution::Command("$(bar)".into())))
            ]))]
        );
        test_commands!(
            "diff $(ls /dir1) ` ls /dir2 `",
            [Sequence::Start(Command(vec![
                Argument::Value(Value::Unquoted("diff".into())),
                Argument::Value(Value::Substitution(Substitution::Command(
                    "$(ls /dir1)".into()
                ))),
                Argument::Value(Value::Substitution(Substitution::Command(
                    "` ls /dir2 `".into()
                )))
            ]))]
        );

        // elvish
        test_commands!(
            "put (echo 'a\nb')",
            [Sequence::Start(Command(vec![
                Argument::Value(Value::Unquoted("put".into())),
                Argument::Value(Value::Substitution(Substitution::Command(
                    "(echo 'a\nb')".into()
                )))
            ]))]
        );
    }

    #[test]
    fn process_substitution() {
        test_commands!(
            "echo <(foo)",
            [Sequence::Start(Command(vec![
                Argument::Value(Value::Unquoted("echo".into())),
                Argument::Value(Value::Substitution(Substitution::Process("<(foo)".into())))
            ]))]
        );
        test_commands!(
            "echo >(bar)",
            [Sequence::Start(Command(vec![
                Argument::Value(Value::Unquoted("echo".into())),
                Argument::Value(Value::Substitution(Substitution::Process(">(bar)".into())))
            ]))]
        );
        test_commands!(
            "diff <(ls /dir1) <( ls /dir2 )",
            [Sequence::Start(Command(vec![
                Argument::Value(Value::Unquoted("diff".into())),
                Argument::Value(Value::Substitution(Substitution::Process(
                    "<(ls /dir1)".into()
                ))),
                Argument::Value(Value::Substitution(Substitution::Process(
                    "<( ls /dir2 )".into()
                )))
            ]))]
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
            Some(Value::SpecialSingleQuoted("another value".into())),
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
        test_args!(
            "KEY=$(echo foo)",
            [Argument::EnvVar(
                "KEY".into(),
                Value::Substitution(Substitution::Command("$(echo foo)".into())),
                None
            )]
        );
        test_args!(
            "KEY=${param}",
            [Argument::EnvVar(
                "KEY".into(),
                Value::Expansion(Expansion::Param("${param}".into())),
                None
            )]
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
                Argument::Option("--d".into(), Some(Value::SpecialSingleQuoted("d".into())))
            ]
        );
        test_args!(
            "--opt=$(echo foo)",
            [Argument::Option(
                "--opt".into(),
                Some(Value::Substitution(Substitution::Command(
                    "$(echo foo)".into()
                ))),
            )]
        );
        test_args!(
            "--opt=${param}",
            [Argument::Option(
                "--opt".into(),
                Some(Value::Expansion(Expansion::Param("${param}".into()))),
            )]
        );
    }
}

mod value {
    use super::*;

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
    fn single_special_quote() {
        test_args!(
            "bin $''",
            [
                Argument::Value(Value::Unquoted("bin".into())),
                Argument::Value(Value::SpecialSingleQuoted("".into()))
            ]
        );
        test_args!(
            "bin $'abc'",
            [
                Argument::Value(Value::Unquoted("bin".into())),
                Argument::Value(Value::SpecialSingleQuoted("abc".into()))
            ]
        );
        test_args!(
            "bin $'a b c'",
            [
                Argument::Value(Value::Unquoted("bin".into())),
                Argument::Value(Value::SpecialSingleQuoted("a b c".into()))
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
    fn single_double_quote() {
        test_args!(
            "bin $\"\"",
            [
                Argument::Value(Value::Unquoted("bin".into())),
                Argument::Value(Value::SpecialDoubleQuoted("".into()))
            ]
        );
        test_args!(
            "bin $\"abc\"",
            [
                Argument::Value(Value::Unquoted("bin".into())),
                Argument::Value(Value::SpecialDoubleQuoted("abc".into()))
            ]
        );
        test_args!(
            "bin $\"a b c\"",
            [
                Argument::Value(Value::Unquoted("bin".into())),
                Argument::Value(Value::SpecialDoubleQuoted("a b c".into()))
            ]
        );
    }

    #[test]
    fn brace_expansion() {
        test_args!(
            "echo a{d,c,b}e",
            [
                Argument::Value(Value::Unquoted("echo".into())),
                Argument::Value(Value::Expansion(Expansion::Brace("a{d,c,b}e".into())))
            ]
        );
        test_args!(
            "mkdir /usr/local/src/bash/{old,new,dist,bugs}",
            [
                Argument::Value(Value::Unquoted("mkdir".into())),
                Argument::Value(Value::Expansion(Expansion::Brace(
                    "/usr/local/src/bash/{old,new,dist,bugs}".into()
                )))
            ]
        );
        test_args!(
            "chown root /usr/{ucb/{ex,edit},lib/{ex?.?*,how_ex}}",
            [
                Argument::Value(Value::Unquoted("chown".into())),
                Argument::Value(Value::Unquoted("root".into())),
                Argument::Value(Value::Expansion(Expansion::Mixed(
                    "/usr/{ucb/{ex,edit},lib/{ex?.?*,how_ex}}".into()
                )))
            ]
        );
        test_args!(
            "echo {1..3}",
            [
                Argument::Value(Value::Unquoted("echo".into())),
                Argument::Value(Value::Expansion(Expansion::Brace("{1..3}".into())))
            ]
        );

        // Not expanded
        test_args!(
            "echo ${test}",
            [
                Argument::Value(Value::Unquoted("echo".into())),
                Argument::Value(Value::Expansion(Expansion::Param("${test}".into())))
            ]
        );
        test_args!(
            "echo foo\\{bar",
            [
                Argument::Value(Value::Unquoted("echo".into())),
                Argument::Value(Value::Unquoted("foo\\{bar".into()))
            ]
        );

        // elvish
        test_args!(
            "put {a b} {1 2}",
            [
                Argument::Value(Value::Unquoted("put".into())),
                Argument::Value(Value::Expansion(Expansion::Brace("{a b}".into()))),
                Argument::Value(Value::Expansion(Expansion::Brace("{1 2}".into()))),
            ]
        );
    }

    #[test]
    fn tilde_expansion() {
        for cmd in [
            "~",
            "~/foo",
            "~fred/foo",
            "~+/foo",
            "~-/foo",
            "~1",
            "~+2",
            "~-3",
        ] {
            test_args!(
                cmd,
                [Argument::Value(Value::Expansion(Expansion::Tilde(
                    cmd.into()
                )))]
            );
        }
    }

    #[test]
    fn param() {
        for param in ["$foo", "$BAR", "$a", "$_", "$fooBAR", "$foo_bar", "$foo123"] {
            test_args!(
                format!("echo {param}"),
                [
                    Argument::Value(Value::Unquoted("echo".into())),
                    Argument::Value(Value::Expansion(Expansion::Param(param.into())))
                ]
            );
        }
    }

    #[test]
    fn param_expansion() {
        for param in [
            "${parameter}",
            "${parameter:−word}",
            "${parameter:=word}",
            "${parameter:?word}",
            "${parameter:+word}",
            "${parameter:offset}",
            "${parameter:offset:length}",
            "${!prefix*}",
            "${!prefix@}",
            "${!name[@]}",
            "${!name[*]}",
            "${#parameter}",
            "${parameter#word}",
            "${parameter##word}",
            "${parameter%word}",
            "${parameter%%word}",
            "${parameter//pattern/string}",
            "${parameter/#pattern/string}",
            "${parameter/%pattern/string}",
            "${parameter^pattern}",
            "${parameter^^pattern}",
            "${parameter,pattern}",
            "${parameter,,pattern}",
            "${parameter@operator}",
        ] {
            test_args!(
                format!("echo {param}"),
                [
                    Argument::Value(Value::Unquoted("echo".into())),
                    Argument::Value(Value::Expansion(Expansion::Param(param.into())))
                ]
            );
        }
    }

    #[test]
    fn filename_expansion() {
        test_args!(
            "echo *",
            [
                Argument::Value(Value::Unquoted("echo".into())),
                Argument::Value(Value::Expansion(Expansion::Wildcard("*".into())))
            ]
        );
        test_args!(
            "ls *.txt",
            [
                Argument::Value(Value::Unquoted("ls".into())),
                Argument::Value(Value::Expansion(Expansion::Wildcard("*.txt".into())))
            ]
        );
        test_args!(
            "ls file?",
            [
                Argument::Value(Value::Unquoted("ls".into())),
                Argument::Value(Value::Expansion(Expansion::Wildcard("file?".into())))
            ]
        );
        test_args!(
            "ls file[1-3].txt",
            [
                Argument::Value(Value::Unquoted("ls".into())),
                Argument::Value(Value::Expansion(Expansion::Wildcard(
                    "file[1-3].txt".into()
                )))
            ]
        );
    }

    #[test]
    fn arithmetic_expansion() {
        test_args!(
            "echo $((2+2))",
            [
                Argument::Value(Value::Unquoted("echo".into())),
                Argument::Value(Value::Expansion(Expansion::Arithmetic("$((2+2))".into())))
            ]
        );
        test_args!(
            "echo $(( (5*4) / 2 ))",
            [
                Argument::Value(Value::Unquoted("echo".into())),
                Argument::Value(Value::Expansion(Expansion::Arithmetic(
                    "$(( (5*4) / 2 ))".into()
                )))
            ]
        );
    }
}

mod shells {
    use super::*;

    #[test]
    fn elvish() {
        // https://elv.sh/ref/language.html#ordinary-command
        test_args!(
            "echo &sep=, a b c",
            [
                Argument::Value(Value::Unquoted("echo".into())),
                Argument::Option("&sep".into(), Some(Value::Unquoted(",".into()))),
                Argument::Value(Value::Unquoted("a".into())),
                Argument::Value(Value::Unquoted("b".into())),
                Argument::Value(Value::Unquoted("c".into())),
            ]
        );

        // https://elv.sh/ref/language.html#pipeline-exception
        test_commands!(
            "while $true { put foo } > run &-",
            [
                Sequence::Start(Command(vec![
                    Argument::Value(Value::Unquoted("while".into())),
                    Argument::Value(Value::Expansion(Expansion::Param("$true".into()))),
                    Argument::Value(Value::Expansion(Expansion::Brace("{ put foo }".into()))),
                ])),
                Sequence::Redirect(
                    Command(vec![Argument::Value(Value::Unquoted("run".into())),]),
                    ">".into()
                ),
                Sequence::Stop("&-".into()),
            ]
        );
    }

    #[test]
    fn fish() {
        // https://fishshell.com/docs/current/language.html#combining-pipes-and-redirections
        test_pipeline!(
            "print 2>&1 | less",
            CommandLine(vec![
                Pipeline::Start(CommandList(vec![
                    Sequence::Start(Command(vec![Argument::Value(Value::Unquoted(
                        "print".into()
                    ))])),
                    Sequence::Stop("2>&1".into()),
                ])),
                Pipeline::Pipe(CommandList(vec![Sequence::Start(Command(vec![
                    Argument::Value(Value::Unquoted("less".into()))
                ])),]))
            ])
        );
        test_commands!(
            "print > /dev/null 2>&1",
            [
                Sequence::Start(Command(vec![Argument::Value(Value::Unquoted(
                    "print".into()
                ))])),
                Sequence::Redirect(
                    Command(vec![Argument::Value(Value::Unquoted("/dev/null".into()))]),
                    ">".into()
                ),
                Sequence::Stop("2>&1".into()),
            ]
        );

        // https://fishshell.com/docs/current/language.html#dereferencing-variables
        test_args!(
            "echo $$var[2][3]",
            [
                Argument::Value(Value::Unquoted("echo".into())),
                Argument::Value(Value::Expansion(Expansion::Mixed("$$var[2][3]".into()))),
            ]
        );
        // NOTE: this is technically wrong since it implies a space!
        // test_args!(
        //     "echo (basename image.jpg .jpg).png",
        //     [
        //         Argument::Value(Value::Unquoted("echo".into())),
        //         Argument::Value(Value::Substitution(Substitution::Command(
        //             "(basename image.jpg .jpg)".into()
        //         ))),
        //         Argument::Value(Value::Unquoted(".png".into())),
        //     ]
        // );
    }

    #[test]
    fn ion() {
        // https://doc.redox-os.org/ion-manual/variables/00-variables.html
        test_args!(
            "echo @array_variable",
            [
                Argument::Value(Value::Unquoted("echo".into())),
                Argument::Value(Value::Expansion(Expansion::Param("@array_variable".into()))),
            ]
        );

        // https://doc.redox-os.org/ion-manual/pipelines.html#detaching-processes
        test_commands!(
            "command &!",
            [
                Sequence::Start(Command(vec![Argument::Value(Value::Unquoted(
                    "command".into()
                ))])),
                Sequence::Stop("&!".into()),
            ]
        );
    }

    #[test]
    fn murex() {
        // https://murex.rocks/parser/brace-quote.html#as-a-function
        test_args!(
            "echo %(hello world)",
            [
                Argument::Value(Value::Unquoted("echo".into())),
                Argument::Value(Value::MurexBraceQuoted("hello world".into())),
            ]
        );
    }

    #[test]
    fn nu() {
        // Note: not exactly accurate!
        test_pipeline!(
            "command out+err>| less",
            CommandLine(vec![Pipeline::Start(CommandList(vec![
                Sequence::Start(Command(vec![Argument::Value(Value::Unquoted(
                    "command".into()
                ))])),
                Sequence::Redirect(
                    Command(vec![Argument::Value(Value::Unquoted("less".into()))]),
                    "out+err>|".into()
                ),
            ]))])
        );

        test_args!(
            "each { $in.name }",
            [
                Argument::Value(Value::Unquoted("each".into())),
                Argument::Value(Value::Expansion(Expansion::Brace("{ $in.name }".into()))),
            ]
        );

        // https://www.nushell.sh/book/working_with_strings.html#raw-strings
        test_args!(
            "echo r#'hello world'#",
            [
                Argument::Value(Value::Unquoted("echo".into())),
                Argument::Value(Value::NuRawQuoted("hello world".into())),
            ]
        );
    }

    #[test]
    fn xonsh() {
        // https://xon.sh/tutorial.html#captured-subprocess-with-and
        test_args!(
            "echo !(ls nonexistent_directory)",
            [
                Argument::Value(Value::Unquoted("echo".into())),
                Argument::Value(Value::Substitution(Substitution::Command(
                    "!(ls nonexistent_directory)".into()
                ))),
            ]
        );
    }

    #[test]
    fn pwsh() {
        // https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_redirection?view=powershell-7.5#examples
        test_commands!(
            "dir C:\\, fakepath 2>&1 .\\dir.log",
            [
                Sequence::Start(Command(vec![
                    Argument::Value(Value::Unquoted("dir".into())),
                    Argument::Value(Value::Unquoted("C:\\,".into())),
                    Argument::Value(Value::Unquoted("fakepath".into())),
                ])),
                Sequence::Redirect(
                    Command(vec![Argument::Value(Value::Unquoted(".\\dir.log".into())),]),
                    "2>&1".into()
                ),
            ]
        );
    }
}
