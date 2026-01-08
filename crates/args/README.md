# starbase_args

![Crates.io](https://img.shields.io/crates/v/starbase_args)
![Crates.io](https://img.shields.io/crates/d/starbase_args)

A generic command line parser with support for POSIX-based shells and more.

This is more than just an argument parser; it supports full command line syntax including piping,
redirection, expansion, and substitution. It organizes parsed tokens into a structured format using
Rust enums and structs.

For example, the command `git rebase -i --empty=drop --exec "echo" HEAD~3` would be parsed into:

```rust
CommandLine(vec![
	Pipeline::Start(CommandList(vec![
		Sequence::Start(Command(vec![
			Argument::Value(Value::Unquoted("git".into())),
			Argument::Value(Value::Unquoted("rebase".into())),
			Argument::Flag("-i".into()),
			Argument::Option("--empty".into(), Some(Value::Unquoted("drop".into()))),
			Argument::Option("--exec".into(), None),
			Argument::Value(Value::DoubleQuoted("echo".into())),
			Argument::Value(Value::Unquoted("HEAD~3".into())),
		]))
	]))
])
```
