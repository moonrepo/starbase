// https://www.gnu.org/software/bash/manual/html_node/index.html#SEC_Contents

use pest::{Parser, iterators::Pair};
use pest_derive::Parser;
use std::fmt;

#[derive(Parser)]
#[grammar = "syntax.pest"]
pub struct ArgsParser;

#[derive(Debug, PartialEq)]
pub enum Value {
    AnsiQuoted(String),
    DoubleQuoted(String),
    SingleQuoted(String),
    Unquoted(String),
}

impl Default for Value {
    fn default() -> Self {
        Value::DoubleQuoted(String::new())
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AnsiQuoted(inner) => write!(f, "$'{inner}'"),
            Self::DoubleQuoted(inner) => write!(f, "\'{inner}\'"),
            Self::SingleQuoted(inner) => write!(f, "'{inner}'"),
            Self::Unquoted(inner) => write!(f, "{inner}"),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Argument {
    // KEY=value, $env:KEY=value
    EnvVar(String, Value, Option<String>),
    // -abc
    FlagGroup(String),
    // -a
    Flag(String),
    // --opt, --opt=value
    Option(String, Option<Value>),
    // value
    Value(Value),
}

impl fmt::Display for Argument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EnvVar(key, value, namespace) => write!(
                f,
                "{}{key}={value};",
                namespace.as_deref().unwrap_or_default()
            ),
            Self::FlagGroup(flag) | Self::Flag(flag) => write!(f, "{flag}"),
            Self::Option(option, value) => match value {
                Some(value) => write!(f, "{option}={value}"),
                None => write!(f, "{option}"),
            },
            Self::Value(value) => write!(f, "{value}"),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Command(pub Vec<Argument>);

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            self.0
                .iter()
                .map(|arg| arg.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        )
    }
}

#[derive(Debug, PartialEq)]
pub enum Sequence {
    Start(Command),
    // ;
    Then(Command),
    // &&
    AndThen(Command),
    // ||
    OrElse(Command),
    // ;, &, etc
    Stop(String),
}

impl fmt::Display for Sequence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Start(command) => write!(f, "{command}"),
            Self::Then(command) => write!(f, "; {command}"),
            Self::AndThen(command) => write!(f, " && {command}"),
            Self::OrElse(command) => write!(f, " || {command}"),
            Self::Stop(term) => {
                if term == ";" {
                    write!(f, ";")
                } else {
                    write!(f, " {term}")
                }
            }
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct CommandList(pub Vec<Sequence>);

impl fmt::Display for CommandList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            self.0
                .iter()
                .map(|seq| seq.to_string())
                .collect::<Vec<_>>()
                .join("")
        )
    }
}

#[derive(Debug, PartialEq)]
pub enum Pipeline {
    Start(CommandList),
    // |
    Pipe(CommandList),
    // |&
    PipeAll(CommandList),
    // ...
    // Redirect(Sequence, String),
}

impl fmt::Display for Pipeline {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Start(command) => write!(f, "{command}"),
            Self::Pipe(command) => write!(f, " | {command}"),
            Self::PipeAll(command) => write!(f, " |& {command}"),
            // Self::Redirect(command, op) => write!(f, " {op} {command}"),
        }
    }
}

fn parse_value(pair: Pair<'_, Rule>) -> Value {
    match pair.as_rule() {
        Rule::value_ansi_quote => Value::AnsiQuoted(
            pair.as_str()
                .trim_start_matches("$'")
                .trim_end_matches("'")
                .into(),
        ),
        Rule::value_double_quote => Value::DoubleQuoted(pair.as_str().trim_matches('"').into()),
        Rule::value_single_quote => Value::SingleQuoted(pair.as_str().trim_matches('\'').into()),
        Rule::value_unquoted => Value::Unquoted(pair.as_str().into()),
        _ => unreachable!(),
    }
}

fn parse_argument(pair: Pair<'_, Rule>) -> Argument {
    match pair.as_rule() {
        // Values
        Rule::value_ansi_quote
        | Rule::value_double_quote
        | Rule::value_single_quote
        | Rule::value_unquoted => Argument::Value(parse_value(pair)),

        // Env vars
        Rule::env_var => {
            let mut inner = pair.into_inner();
            let mut namespace = None;

            if inner.len() == 3 {
                namespace = Some(
                    inner
                        .next()
                        .expect("Missing env var namespace!")
                        .as_str()
                        .to_owned(),
                );
            }

            let key = inner.next().expect("Missing env var key!");
            let value = inner.next().expect("Missing env var value!");

            Argument::EnvVar(key.as_str().into(), parse_value(value), namespace)
        }

        // Flags
        Rule::flag_group => Argument::FlagGroup(pair.as_str().into()),
        Rule::flag => Argument::Flag(pair.as_str().into()),

        // Options
        Rule::option => Argument::Option(pair.as_str().into(), None),
        Rule::option_with_value => {
            let mut inner = pair.into_inner();
            let key = inner.next().expect("Missing option key!");
            let value = inner.next().expect("Missing option value!");

            Argument::Option(key.as_str().into(), Some(parse_value(value)))
        }

        _ => unimplemented!(),
    }
}

fn parse_command(pair: Pair<'_, Rule>) -> Command {
    match pair.as_rule() {
        Rule::command => {
            let mut args = vec![];

            for inner in pair.into_inner() {
                args.push(parse_argument(inner));
            }

            Command(args)
        }
        _ => unimplemented!(),
    }
}

fn parse_command_list(pair: Pair<'_, Rule>) -> CommandList {
    match pair.as_rule() {
        Rule::command_list => {
            let mut list = vec![];
            let mut last_command = None;
            let mut last_separator = None;

            for inner in pair.into_inner() {
                match inner.as_rule() {
                    Rule::command => {
                        if let Some(command) = last_command.take() {
                            match last_separator.take() {
                                Some("&&") => {
                                    list.push(Sequence::AndThen(command));
                                }
                                Some("||") => {
                                    list.push(Sequence::OrElse(command));
                                }
                                _ => {
                                    if list.is_empty() {
                                        list.push(Sequence::Start(command));
                                    } else {
                                        list.push(Sequence::Then(command));
                                    }
                                }
                            };
                        }

                        last_command = Some(parse_command(inner));
                    }
                    Rule::command_separator => {
                        last_separator = Some(inner.as_str());
                    }
                    Rule::command_terminator => {
                        if let Some(command) = last_command.take() {
                            list.push(Sequence::Then(command));
                        }

                        list.push(Sequence::Stop(inner.as_str().into()));
                    }
                    _ => unimplemented!(),
                };
            }

            if let Some(command) = last_command.take() {
                list.push(Sequence::Then(command));
            }

            CommandList(list)
        }
        _ => unimplemented!(),
    }
}

fn parse_pipeline(pair: Pair<'_, Rule>) -> Vec<Pipeline> {
    match pair.as_rule() {
        Rule::pipeline => {
            let mut list = vec![];
            let mut last_command_list = None;
            let mut last_separator = None;

            for inner in pair.into_inner() {
                match inner.as_rule() {
                    Rule::command_list => {
                        if let Some(command_list) = last_command_list.take() {
                            match last_separator.take() {
                                Some("|") => {
                                    list.push(Pipeline::Pipe(command_list));
                                }
                                Some("|&") => {
                                    list.push(Pipeline::PipeAll(command_list));
                                }
                                _ => {
                                    list.push(Pipeline::Start(command_list));
                                }
                            };
                        }

                        last_command_list = Some(parse_command_list(inner));
                    }
                    Rule::pipeline_separator => {
                        last_separator = Some(inner.as_str());
                    }
                    _ => unreachable!(),
                };
            }

            if let Some(command_list) = last_command_list.take() {
                list.push(Pipeline::Pipe(command_list));
            }

            list
        }
        _ => unimplemented!(),
    }
}

pub fn parse_args<T: AsRef<str>>(input: T) -> Vec<Argument> {
    let a = ArgsParser::parse(Rule::args, input.as_ref().trim());
    dbg!(&a);
    let pairs = a.unwrap();
    let mut args = vec![];

    for pair in pairs {
        if pair.as_rule() == Rule::pipeline {
            // args.push(arg);
        }
    }

    args
}

pub fn parse<T: AsRef<str>>(input: T) -> Vec<Pipeline> {
    let pairs = ArgsParser::parse(Rule::args, input.as_ref().trim()).unwrap();
    let mut pipeline = vec![];

    for pair in pairs {
        if pair.as_rule() == Rule::pipeline {
            pipeline.extend(parse_pipeline(pair));
        }
    }

    pipeline
}
