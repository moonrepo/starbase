use pest::{Parser, iterators::Pair};
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "syntax.pest"]
pub struct ArgsParser;

#[derive(Debug, PartialEq)]
pub enum Value {
    SpecialQuoted(String),
    DoubleQuoted(String),
    SingleQuoted(String),
    Unquoted(String),
}

#[derive(Debug, PartialEq)]
pub enum Argument {
    Executable(Value),
    FlagGroup(String),
    Flag(String),
    Option(String, Option<Value>),
    Value(Value),
}

fn parse_value(pair: Pair<'_, Rule>) -> Value {
    match pair.as_rule() {
        Rule::value_bash_quote => Value::SpecialQuoted(pair.as_str().into()),
        Rule::value_double_quote => Value::DoubleQuoted(pair.as_str().into()),
        Rule::value_single_quote => Value::SingleQuoted(pair.as_str().into()),
        Rule::value_unquoted => Value::Unquoted(pair.as_str().into()),
        Rule::value => parse_value(pair.into_inner().next().expect("Missing value!")),
        _ => unreachable!(),
    }
}

fn parse_pair(pair: Pair<'_, Rule>, index: usize) -> Option<Argument> {
    dbg!(pair.as_str(), pair.as_rule());

    let arg = match pair.as_rule() {
        Rule::value_bash_quote
        | Rule::value_double_quote
        | Rule::value_single_quote
        | Rule::value_unquoted
        | Rule::value => {
            if index == 0 {
                Argument::Executable(parse_value(pair))
            } else {
                Argument::Value(parse_value(pair))
            }
        }
        Rule::flag_group => Argument::FlagGroup(pair.as_str().into()),
        Rule::flag => Argument::Flag(pair.as_str().into()),
        Rule::option => Argument::Option(pair.as_str().into(), None),
        Rule::option_with_value => {
            let mut inner = pair.into_inner();
            let key = inner.next().expect("Missing option key!");
            let value = inner.next().expect("Missing option value!");

            Argument::Option(key.as_str().into(), Some(parse_value(value)))
        }
        _ => return None,
    };

    Some(arg)
}

pub fn parse_args(input: &str) -> Vec<Argument> {
    let pairs = ArgsParser::parse(Rule::args, input).unwrap();
    let mut args = vec![];
    let mut index = 0;

    for pair in pairs {
        if let Some(arg) = parse_pair(pair, index) {
            args.push(arg);
        }

        index += 1;
    }

    args
}
