// https://www.gnu.org/software/bash/manual/html_node/index.html#SEC_Contents

use pest::{Parser, iterators::Pair};
use pest_derive::Parser;
use std::fmt;
use std::ops::Deref;

pub use pest::error::*;

#[derive(Parser)]
#[grammar = "syntax.pest"]
pub struct ArgsParser;

#[derive(Debug, PartialEq)]
pub enum Expansion {
    /// $(())
    Arithmetic(String),
    /// {}
    Brace(String),
    /// ...
    Mixed(String),
    /// ${}, $param
    Param(String),
    /// ~
    Tilde(String),
    /// @token(id)
    TokenFunc(String),
    /// *, ?, []
    Wildcard(String),
}

impl fmt::Display for Expansion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arithmetic(inner)
            | Self::Brace(inner)
            | Self::Wildcard(inner)
            | Self::Param(inner)
            | Self::Mixed(inner)
            | Self::TokenFunc(inner)
            | Self::Tilde(inner) => write!(f, "{inner}"),
        }
    }
}

impl Expansion {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Arithmetic(inner)
            | Self::Brace(inner)
            | Self::Mixed(inner)
            | Self::Param(inner)
            | Self::Tilde(inner)
            | Self::TokenFunc(inner)
            | Self::Wildcard(inner) => inner,
        }
    }

    fn detect(value: &str) -> Option<Self> {
        let mut found = vec![];
        let mut last_ch = ' ';
        let mut in_bracket = false;

        for ch in value.chars() {
            // https://www.gnu.org/software/bash/manual/html_node/Brace-Expansion.html
            if ch == '{' && last_ch != '$' && last_ch != '\\' {
                found.push(Self::Brace(value.into()));
            }

            if ch == '{' {
                in_bracket = true;
            } else if ch == '}' {
                in_bracket = false;
            }

            // https://www.gnu.org/software/bash/manual/html_node/Filename-Expansion.html
            if (ch == '*'
                || ch == '?'
                || ch == '['
                || (ch == '('
                    && (last_ch == '?'
                        || last_ch == '*'
                        || last_ch == '+'
                        || last_ch == '@'
                        || last_ch == '!')))
                && !in_bracket
            {
                found.push(Self::Wildcard(value.into()));
            }

            last_ch = ch;
        }

        if found.is_empty() {
            None
        } else if found.len() > 1 {
            Some(Self::Mixed(value.into()))
        } else {
            Some(found.remove(0))
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Substitution {
    /// $(), ()
    Command(String),
    /// <(), >()
    Process(String),
}

impl fmt::Display for Substitution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Command(inner) | Self::Process(inner) => write!(f, "{inner}"),
        }
    }
}

impl Substitution {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Command(inner) | Self::Process(inner) => inner,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Value {
    /// ""
    DoubleQuoted(String),
    /// $""
    SpecialDoubleQuoted(String),
    /// ''
    SingleQuoted(String),
    /// $''
    SpecialSingleQuoted(String),
    /// ...
    Unquoted(String),
    /// $(()), ${}, {}, ...
    Expansion(Expansion),
    /// $(), ...
    Substitution(Substitution),

    /// %()
    MurexBraceQuoted(String),
    /// r#''#
    NuRawQuoted(String),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DoubleQuoted(inner) => write!(f, "\"{inner}\""),
            Self::SpecialDoubleQuoted(inner) => write!(f, "$\"{inner}\""),
            Self::SingleQuoted(inner) => write!(f, "'{inner}'"),
            Self::SpecialSingleQuoted(inner) => write!(f, "$'{inner}'"),
            Self::Unquoted(inner) => write!(f, "{inner}"),
            Self::Expansion(inner) => write!(f, "{inner}"),
            Self::Substitution(inner) => write!(f, "{inner}"),
            Self::MurexBraceQuoted(inner) => write!(f, "%({inner})"),
            Self::NuRawQuoted(inner) => write!(f, "r#'{inner}'#"),
        }
    }
}

impl Value {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Unquoted(inner) => inner,
            Self::Expansion(expansion) => expansion.as_str(),
            Self::Substitution(substitution) => substitution.as_str(),
            _ => self.get_quoted_value(),
        }
    }

    pub fn is_quoted(&self) -> bool {
        matches!(
            self,
            Self::DoubleQuoted(_)
                | Self::SpecialDoubleQuoted(_)
                | Self::SingleQuoted(_)
                | Self::SpecialSingleQuoted(_)
                | Self::MurexBraceQuoted(_)
                | Self::NuRawQuoted(_)
        )
    }

    /// If the value is quoted, returns the value within the quotes.
    /// Otherwise returns an empty string.
    pub fn get_quoted_value(&self) -> &str {
        match self {
            Self::DoubleQuoted(inner)
            | Self::SpecialDoubleQuoted(inner)
            | Self::SingleQuoted(inner)
            | Self::SpecialSingleQuoted(inner)
            | Self::MurexBraceQuoted(inner)
            | Self::NuRawQuoted(inner) => inner,
            _ => "",
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Argument {
    /// KEY=value, $env:KEY=value
    EnvVar(String, Value, Option<String>),
    /// -abc
    FlagGroup(String),
    /// -a
    Flag(String),
    /// --opt, --opt=value
    Option(String, Option<Value>),
    /// value
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

impl Deref for Command {
    type Target = Vec<Argument>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, PartialEq)]
pub enum Sequence {
    Start(Command),
    /// ;
    Then(Command),
    /// &&
    AndThen(Command),
    /// ||
    OrElse(Command),
    /// --
    Passthrough(Command),
    /// >, <, etc
    Redirect(Command, String),
    /// ;, &, etc
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

impl Deref for CommandList {
    type Target = Vec<Sequence>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, PartialEq)]
pub enum Pipeline {
    Start(CommandList),
    /// !
    StartNegated(CommandList),
    /// |
    Pipe(CommandList),
    /// |&
    PipeAll(CommandList),
    /// ...
    PipeWith(CommandList, String),
}

impl fmt::Display for Pipeline {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Start(command) => write!(f, "{command}"),
            Self::StartNegated(command) => write!(f, "! {command}"),
            Self::Pipe(command) => write!(f, " | {command}"),
            Self::PipeAll(command) => write!(f, " |& {command}"),
            Self::PipeWith(command, op) => write!(f, " {op} {command}"),
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

impl Deref for CommandLine {
    type Target = Vec<Pipeline>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

fn handle_unquoted_value(pair: Pair<'_, Rule>) -> Value {
    let inner = pair.as_str().trim();

    if let Ok(value) = parse_unquoted_value(inner) {
        return value;
    }

    if let Some(value) = Expansion::detect(inner) {
        return Value::Expansion(value);
    }

    Value::Unquoted(inner.into())
}

fn handle_value(pair: Pair<'_, Rule>) -> Value {
    let inner = pair.as_str().trim();

    match pair.as_rule() {
        Rule::value_unquoted => handle_unquoted_value(pair),

        Rule::value_murex_brace_quote => {
            Value::MurexBraceQuoted(inner.trim_start_matches("%(").trim_end_matches(")").into())
        }

        Rule::value_nu_raw_quote => Value::NuRawQuoted(
            inner
                .trim_start_matches("r#'")
                .trim_end_matches("'#")
                .into(),
        ),

        Rule::value_double_quote => {
            if inner.starts_with('$') {
                Value::SpecialDoubleQuoted(
                    inner.trim_start_matches("$\"").trim_end_matches('"').into(),
                )
            } else {
                Value::DoubleQuoted(inner.trim_matches('"').into())
            }
        }

        Rule::value_single_quote => {
            if inner.starts_with('$') {
                Value::SpecialSingleQuoted(
                    inner.trim_start_matches("$'").trim_end_matches('\'').into(),
                )
            } else {
                Value::SingleQuoted(inner.trim_matches('\'').into())
            }
        }

        // Expansions
        Rule::arithmetic_expansion => Value::Expansion(Expansion::Arithmetic(inner.into())),
        Rule::brace_expansion => Value::Expansion(Expansion::Brace(inner.into())),
        Rule::parameter_expansion | Rule::param => {
            if inner.starts_with(['$', '@']) {
                Value::Expansion(Expansion::Param(inner.into()))
            } else {
                Value::Expansion(Expansion::Brace(inner.into()))
            }
        }
        Rule::param_special => Value::Expansion(Expansion::Param(inner.into())),
        Rule::tilde_expansion => Value::Expansion(Expansion::Tilde(inner.into())),
        Rule::moon_token_expansion => Value::Expansion(Expansion::TokenFunc(inner.into())),

        // Substitution
        Rule::command_substitution => Value::Substitution(Substitution::Command(inner.into())),
        Rule::process_substitution => Value::Substitution(Substitution::Process(inner.into())),

        _ => unreachable!(),
    }
}

fn handle_argument(pair: Pair<'_, Rule>) -> Option<Argument> {
    let arg = match pair.as_rule() {
        // Values
        Rule::value_murex_brace_quote
        | Rule::value_nu_raw_quote
        | Rule::value_double_quote
        | Rule::value_single_quote
        | Rule::value_unquoted
        | Rule::arithmetic_expansion
        | Rule::brace_expansion
        | Rule::parameter_expansion
        | Rule::tilde_expansion
        | Rule::moon_token_expansion
        | Rule::param
        | Rule::param_special
        | Rule::command_substitution
        | Rule::process_substitution => Argument::Value(handle_value(pair)),

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

            Argument::EnvVar(key.as_str().into(), handle_value(value), namespace)
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

            Argument::Option(key.as_str().into(), Some(handle_value(value)))
        }

        _ => return None,
    };

    Some(arg)
}

fn handle_command(pair: Pair<'_, Rule>) -> Command {
    match pair.as_rule() {
        Rule::command => {
            let mut args = vec![];

            for inner in pair.into_inner() {
                if let Some(arg) = handle_argument(inner) {
                    args.push(arg);
                }
            }

            Command(args)
        }
        _ => unreachable!(),
    }
}

fn handle_command_list(pair: Pair<'_, Rule>) -> CommandList {
    match pair.as_rule() {
        Rule::command_list => {
            let mut list = vec![];
            let mut control_operator: Option<&str> = None;
            let mut redirect_operator: Option<&str> = None;

            for inner in pair.into_inner() {
                match inner.as_rule() {
                    Rule::command => {
                        let command = handle_command(inner);

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

fn handle_pipeline(pair: Pair<'_, Rule>) -> Vec<Pipeline> {
    match pair.as_rule() {
        Rule::pipeline => {
            let mut list = vec![];
            let mut last_command_list = None;
            let mut last_operator = None;
            let mut negated = false;

            for inner in pair.into_inner() {
                match inner.as_rule() {
                    Rule::command_list => {
                        let command_list = handle_command_list(inner);

                        if list.is_empty() {
                            if negated {
                                list.push(Pipeline::StartNegated(command_list));
                            } else {
                                list.push(Pipeline::Start(command_list));
                            }
                        } else {
                            match last_operator.take() {
                                Some("|") | None => {
                                    list.push(Pipeline::Pipe(command_list));
                                }
                                Some("|&") => {
                                    list.push(Pipeline::PipeAll(command_list));
                                }
                                Some(op) => {
                                    list.push(Pipeline::PipeWith(command_list, op.into()));
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

#[allow(clippy::result_large_err)]
pub fn parse<T: AsRef<str>>(input: T) -> Result<CommandLine, pest::error::Error<Rule>> {
    let pairs = ArgsParser::parse(Rule::command_line, input.as_ref().trim())?;
    let mut pipeline = vec![];

    for pair in pairs {
        if pair.as_rule() == Rule::pipeline {
            pipeline.extend(handle_pipeline(pair));
        }
    }

    Ok(CommandLine(pipeline))
}

#[allow(clippy::result_large_err)]
pub fn parse_unquoted_value<T: AsRef<str>>(input: T) -> Result<Value, pest::error::Error<Rule>> {
    let pairs = ArgsParser::parse(
        Rule::unquoted_expansion_or_substitution,
        input.as_ref().trim(),
    )?;

    for pair in pairs {
        if pair.as_rule() != Rule::EOI {
            return Ok(handle_value(pair));
        }
    }

    Ok(Value::Unquoted(input.as_ref().into()))
}
