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
    Expansion(Expansion),
    Substitution(Substitution),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AnsiQuoted(inner) => write!(f, "$'{inner}'"),
            Self::DoubleQuoted(inner) => write!(f, "\"{inner}\""),
            Self::SingleQuoted(inner) => write!(f, "'{inner}'"),
            Self::Unquoted(inner) => write!(f, "{inner}"),
            Self::Expansion(inner) => write!(f, "{inner}"),
            Self::Substitution(inner) => write!(f, "{inner}"),
        }
    }
}

pub enum ExpansionType {
    Arithmetic,
    Brace,
    Filename,
    Mixed,
    Param,
    Tilde,
}

impl ExpansionType {
    fn detect(value: &str) -> Option<Self> {
        if value.starts_with('~') {
            return Some(Self::Tilde);
        } else if value.starts_with("$((") {
            return Some(Self::Arithmetic);
        } else if value.starts_with("${") {
            return Some(Self::Param);
        }

        let mut found = vec![];
        let mut last_ch = ' ';

        for ch in value.chars() {
            // https://www.gnu.org/software/bash/manual/html_node/Brace-Expansion.html
            if ch == '{' && last_ch != '$' && last_ch != '\\' {
                found.push(ExpansionType::Brace);
            }

            // https://www.gnu.org/software/bash/manual/html_node/Filename-Expansion.html
            if ch == '*'
                || ch == '?'
                || ch == '['
                || (ch == '('
                    && (last_ch == '?'
                        || last_ch == '*'
                        || last_ch == '+'
                        || last_ch == '@'
                        || last_ch == '!'))
            {
                found.push(ExpansionType::Filename);
            }

            last_ch = ch;
        }

        if found.is_empty() {
            None
        } else if found.len() > 1 {
            Some(Self::Mixed)
        } else {
            Some(found.remove(0))
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Expansion {
    Arithmetic(String),
    Brace(String),
    Filename(String),
    Mixed(String),
    Param(String),
    Tilde(String),
}

impl fmt::Display for Expansion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arithmetic(inner)
            | Self::Brace(inner)
            | Self::Filename(inner)
            | Self::Param(inner)
            | Self::Mixed(inner)
            | Self::Tilde(inner) => write!(f, "{inner}"),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Substitution {
    Command(String),
    Process(String),
}

impl fmt::Display for Substitution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Command(inner) | Self::Process(inner) => write!(f, "{inner}"),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Argument {
    // KEY=value, $env:KEY=value
    EnvVar(String, Value, Option<String>),
    // $(), ${}, ...
    Expansion(Expansion),
    // -abc
    FlagGroup(String),
    // -a
    Flag(String),
    // --opt, --opt=value
    Option(String, Option<Value>),
    // $(), <(), ...
    Substitution(Substitution),
    // value
    Value(Value),
}

impl fmt::Display for Argument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EnvVar(key, value, namespace) => write!(
                f,
                "{}{key}={value}",
                namespace.as_deref().unwrap_or_default()
            ),
            Self::Expansion(inner) => write!(f, "{inner}"),
            Self::FlagGroup(flag) | Self::Flag(flag) => write!(f, "{flag}"),
            Self::Option(option, value) => match value {
                Some(value) => write!(f, "{option}={value}"),
                None => write!(f, "{option}"),
            },
            Self::Substitution(inner) => write!(f, "{inner}"),
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
    // --
    Passthrough(Command),
    // >, <, etc
    Redirect(Command, String),
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
            Self::Passthrough(command) => write!(f, " -- {command}"),
            Self::Redirect(command, op) => write!(f, " {op} {command}"),
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
    // !
    StartNegated(CommandList),
    // |
    Pipe(CommandList),
    // |&
    PipeAll(CommandList),
}

impl fmt::Display for Pipeline {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Start(command) => write!(f, "{command}"),
            Self::StartNegated(command) => write!(f, "! {command}"),
            Self::Pipe(command) => write!(f, " | {command}"),
            Self::PipeAll(command) => write!(f, " |& {command}"),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct CommandLine(pub Vec<Pipeline>);

impl fmt::Display for CommandLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            self.0
                .iter()
                .map(|arg| arg.to_string())
                .collect::<Vec<_>>()
                .join("")
        )
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

        // Expansions
        Rule::arithmetic_expansion => Value::Expansion(Expansion::Arithmetic(pair.as_str().into())),
        Rule::parameter_expansion | Rule::param => {
            Value::Expansion(Expansion::Param(pair.as_str().into()))
        }

        // Substitution
        Rule::command_substitution => {
            Value::Substitution(Substitution::Command(pair.as_str().into()))
        }
        Rule::process_substitution => {
            Value::Substitution(Substitution::Process(pair.as_str().into()))
        }

        _ => unreachable!(),
    }
}

fn parse_argument(pair: Pair<'_, Rule>) -> Argument {
    match pair.as_rule() {
        // Values
        Rule::value_ansi_quote | Rule::value_double_quote | Rule::value_single_quote => {
            Argument::Value(parse_value(pair))
        }

        Rule::value_unquoted => {
            let inner = pair.as_str();

            if let Some(exp) = ExpansionType::detect(inner) {
                Argument::Expansion(match exp {
                    ExpansionType::Arithmetic => Expansion::Arithmetic(inner.into()),
                    ExpansionType::Brace => Expansion::Brace(inner.into()),
                    ExpansionType::Filename => Expansion::Filename(inner.into()),
                    ExpansionType::Mixed => Expansion::Mixed(inner.into()),
                    ExpansionType::Param => Expansion::Param(inner.into()),
                    ExpansionType::Tilde => Expansion::Tilde(inner.into()),
                })
            } else {
                Argument::Value(parse_value(pair))
            }
        }

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

        // Expansions
        Rule::arithmetic_expansion => {
            Argument::Expansion(Expansion::Arithmetic(pair.as_str().into()))
        }
        Rule::parameter_expansion | Rule::param => {
            Argument::Expansion(Expansion::Param(pair.as_str().into()))
        }

        // Substitution
        Rule::command_substitution => {
            Argument::Substitution(Substitution::Command(pair.as_str().into()))
        }
        Rule::process_substitution => {
            Argument::Substitution(Substitution::Process(pair.as_str().into()))
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

        _ => unreachable!(),
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
        _ => unreachable!(),
    }
}

fn parse_command_list(pair: Pair<'_, Rule>) -> CommandList {
    match pair.as_rule() {
        Rule::command_list => {
            let mut list = vec![];
            let mut control_operator: Option<&str> = None;
            let mut redirect_operator: Option<&str> = None;

            for inner in pair.into_inner() {
                match inner.as_rule() {
                    Rule::command => {
                        let command = parse_command(inner);

                        if list.is_empty() {
                            list.push(Sequence::Start(command));
                        } else if let Some(control) = control_operator.take() {
                            match control {
                                "&&" => {
                                    list.push(Sequence::AndThen(command));
                                }
                                "||" => {
                                    list.push(Sequence::OrElse(command));
                                }
                                "--" => {
                                    list.push(Sequence::Passthrough(command));
                                }
                                _ => {
                                    list.push(Sequence::Then(command));
                                }
                            };
                        } else if let Some(redirect) = redirect_operator.take() {
                            list.push(Sequence::Redirect(command, redirect.into()));
                        } else {
                            list.push(Sequence::Then(command));
                        }
                    }
                    Rule::control_operator => {
                        control_operator = Some(inner.as_str().trim());
                    }
                    Rule::redirect_operator | Rule::redirect_operator_with_fd => {
                        redirect_operator = Some(inner.as_str().trim());
                    }
                    Rule::command_terminator => {
                        list.push(Sequence::Stop(inner.as_str().into()));
                    }
                    _ => unreachable!(),
                };
            }

            CommandList(list)
        }
        _ => unreachable!(),
    }
}

fn parse_pipeline(pair: Pair<'_, Rule>) -> Vec<Pipeline> {
    match pair.as_rule() {
        Rule::pipeline => {
            let mut list = vec![];
            let mut last_command_list = None;
            let mut last_operator = None;
            let mut negated = false;

            for inner in pair.into_inner() {
                match inner.as_rule() {
                    Rule::command_list => {
                        let command_list = parse_command_list(inner);

                        if list.is_empty() {
                            if negated {
                                list.push(Pipeline::StartNegated(command_list));
                            } else {
                                list.push(Pipeline::Start(command_list));
                            }
                        } else {
                            match last_operator.take() {
                                Some("|&") => {
                                    list.push(Pipeline::PipeAll(command_list));
                                }
                                _ => {
                                    list.push(Pipeline::Pipe(command_list));
                                }
                            };
                        }
                    }
                    Rule::pipeline_negated => {
                        negated = true;
                    }
                    Rule::pipeline_operator => {
                        last_operator = Some(inner.as_str());
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

pub fn parse<T: AsRef<str>>(input: T) -> Result<CommandLine, ()> {
    let pairs = ArgsParser::parse(Rule::args, input.as_ref().trim()).unwrap();
    let mut pipeline = vec![];

    for pair in pairs {
        // dbg!(&pair);

        if pair.as_rule() == Rule::pipeline {
            pipeline.extend(parse_pipeline(pair));
        }
    }

    Ok(CommandLine(pipeline))
}
