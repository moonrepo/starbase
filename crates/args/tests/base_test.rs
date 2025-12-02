use starbase_args::{Argument, Value, parse_args};

mod args_base {
    use super::*;

    #[test]
    fn single_quote() {
        assert_eq!(
            parse_args("bin ''"),
            [
                Argument::Executable(Value::Unquoted("bin".into())),
                Argument::Value(Value::SingleQuoted("''".into()))
            ]
        );
        assert_eq!(
            parse_args("bin 'abc'"),
            [
                Argument::Executable(Value::Unquoted("bin".into())),
                Argument::Value(Value::SingleQuoted("'abc'".into()))
            ]
        );
        assert_eq!(
            parse_args("bin 'a b c'"),
            [
                Argument::Executable(Value::Unquoted("bin".into())),
                Argument::Value(Value::SingleQuoted("'a b c'".into()))
            ]
        );
    }

    #[test]
    fn exe() {
        assert_eq!(
            parse_args("bin"),
            [Argument::Executable(Value::Unquoted("bin".into()))]
        );
        assert_eq!(
            parse_args("file.sh"),
            [Argument::Executable(Value::Unquoted("file.sh".into()))]
        );
        assert_eq!(
            parse_args("./file.sh"),
            [Argument::Executable(Value::Unquoted("./file.sh".into()))]
        );
        assert_eq!(
            parse_args("../file.sh"),
            [Argument::Executable(Value::Unquoted("../file.sh".into()))]
        );
        assert_eq!(
            parse_args("\"file with space.sh\""),
            [Argument::Executable(Value::DoubleQuoted(
                "\"file with space.sh\"".into()
            ))]
        );
    }
}
