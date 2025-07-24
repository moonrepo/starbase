use crate::BoxedShell;
use crate::helpers::{quotable_contains, quotable_equals, quotable_into_string};
use shell_quote::Quotable;

/// Join a list of arguments into a command line string using
/// the provided [`Shell`] instance as the quoting mechanism.
pub fn join_args<'a, I, V>(shell: &BoxedShell, args: I) -> String
where
    I: IntoIterator<Item = V>,
    V: Into<Quotable<'a>>,
{
    let mut out = String::new();
    let args = args.into_iter().collect::<Vec<_>>();
    let last_index = args.len() - 1;

    for (index, arg) in args.into_iter().enumerate() {
        let arg = arg.into();

        match ArgSyntax::determine(&arg) {
            ArgSyntax::Text => {
                let quoted_arg = shell.create_quoter(arg.into()).maybe_quote();

                out.push_str(&quoted_arg);
            }
            _ => {
                let unquoted_arg = quotable_into_string(arg);

                out.push_str(&unquoted_arg);
            }
        };

        if index != last_index {
            out.push(' ');
        }
    }

    out
}

#[derive(PartialEq)]
pub enum ArgSyntax {
    Option,
    Operator,
    Pipe,
    Redirection,
    Text,
    TextGlob,
}

impl ArgSyntax {
    pub fn determine(value: &Quotable<'_>) -> ArgSyntax {
        // Option
        match value {
            Quotable::Bytes(bytes) => {
                if bytes.starts_with(b"-") {
                    return ArgSyntax::Option;
                }
            }
            Quotable::Text(text) => {
                if text.starts_with("-") {
                    return ArgSyntax::Option;
                }
            }
        };

        if quotable_equals(value, ["&", "&&", "&!", "||", "!", ";"]) {
            return ArgSyntax::Operator;
        }

        if quotable_equals(value, ["|", "^|", "&|", "|&"]) {
            return ArgSyntax::Pipe;
        }

        if quotable_equals(value, [">", "^>", "&>", ">>", "<", "<<"]) {
            return ArgSyntax::Redirection;
        }

        if quotable_contains(value, ["*", "[", "{", "?"]) {
            return ArgSyntax::TextGlob;
        }

        ArgSyntax::Text
    }
}
