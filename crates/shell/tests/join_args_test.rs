use starbase_shell::{Bash, BoxedShell, join_args};

fn create_bash() -> BoxedShell {
    Box::new(Bash::new())
}

mod join_args {
    use super::*;

    #[test]
    fn empty_args() {
        assert_eq!(join_args(&create_bash(), Vec::<&str>::new()), "");
    }

    #[test]
    fn normal_args() {
        assert_eq!(
            join_args(
                &create_bash(),
                ["bin", "arg1", "arg2", "-o", "--opt", "val"]
            ),
            "bin arg1 arg2 -o --opt val"
        );
    }

    #[test]
    fn with_delim() {
        assert_eq!(
            join_args(&create_bash(), ["bin", "arg1", "--", "extra", "args"]),
            "bin arg1 -- extra args"
        );
    }

    #[test]
    fn quotes() {
        assert_eq!(
            join_args(&create_bash(), ["bin", "foo bar"]),
            "bin $'foo bar'"
        );
    }

    #[test]
    fn quoted_strings() {
        assert_eq!(
            join_args(&create_bash(), ["echo", "'foo'", "\"bar\""]),
            "echo 'foo' \"bar\""
        );
    }

    #[test]
    fn globs_dont_quote() {
        assert_eq!(
            join_args(&create_bash(), ["test", "./tests/*.js"]),
            "test ./tests/*.js"
        );
        assert_eq!(
            join_args(&create_bash(), ["test", "./{test,spec}/**/*.tsx?"]),
            "test ./{test,spec}/**/*.tsx?"
        );
    }

    #[test]
    fn special_chars() {
        assert_eq!(
            join_args(&create_bash(), ["bin", "@dir/path"]),
            "bin $'@dir/path'"
        );
    }

    #[test]
    fn multi_and() {
        assert_eq!(
            join_args(&create_bash(), ["bin1", "arg", "&&", "bin2", "arg"]),
            "bin1 arg && bin2 arg"
        );
    }

    #[test]
    fn multi_semicolon() {
        assert_eq!(
            join_args(&create_bash(), ["bin1", "arg", ";", "bin2", "arg"]),
            "bin1 arg ; bin2 arg"
        );
    }

    #[test]
    fn operators() {
        assert_eq!(
            join_args(&create_bash(), ["bin", "||", "true"]),
            "bin || true"
        );
        assert_eq!(
            join_args(&create_bash(), ["bin", ">", "./file"]),
            "bin > ./file"
        );
        assert_eq!(
            join_args(&create_bash(), ["bin", ">>", "./file"]),
            "bin >> ./file"
        );
        assert_eq!(
            join_args(&create_bash(), ["bin", "|", "bin2"]),
            "bin | bin2"
        );
    }

    #[test]
    fn echo_vars() {
        assert_eq!(
            join_args(&create_bash(), ["echo", "$VAR_NAME"]),
            "echo \"$VAR_NAME\""
        );
    }

    #[test]
    fn quotes_strings_with_dashes() {
        assert_eq!(
            join_args(&create_bash(), ["echo", "some value-with a dash"]),
            "echo $'some value-with a dash'"
        );
    }

    #[test]
    fn expansion_brace() {
        assert_eq!(
            join_args(&create_bash(), ["echo", "a{d,c,b}e"]),
            "echo a{d,c,b}e"
        );
    }

    #[test]
    fn expansion_shell_param() {
        assert_eq!(
            join_args(&create_bash(), ["echo", "${var-unset}"]),
            "echo \"${var-unset}\""
        );

        assert_eq!(
            join_args(&create_bash(), ["echo", "${array[0]:7}"]),
            "echo \"${array[0]:7}\""
        );
    }

    #[test]
    fn expansion_command() {
        assert_eq!(
            join_args(&create_bash(), ["echo", "$(ls)"]),
            "echo \"$(ls)\""
        );

        assert_eq!(
            join_args(&create_bash(), ["echo", "value=$(c git status;)"]),
            "echo \"value=$(c git status;)\""
        );
    }

    #[test]
    fn expansion_tilde() {
        assert_eq!(join_args(&create_bash(), ["echo", "~"]), "echo ~");

        assert_eq!(join_args(&create_bash(), ["echo", "~+/foo"]), "echo ~+/foo");

        assert_eq!(
            join_args(&create_bash(), ["echo", "~fred/foo"]),
            "echo ~fred/foo"
        );

        assert_eq!(
            join_args(&create_bash(), ["echo", "in ~ middle"]),
            "echo $'in ~ middle'"
        );
    }

    #[test]
    fn community_use_cases() {
        assert_eq!(
            join_args(
                &create_bash(),
                [
                    "dotnet",
                    "build",
                    "$projectRoot",
                    "-c:Release",
                    "-p:AssemblyVersion=$VERSION",
                    "--no-dependencies",
                    "--no-restore",
                    "--warnaserror",
                    "--os",
                    "win"
                ]
            ),
            "dotnet build \"$projectRoot\" -c:Release -p:AssemblyVersion=$VERSION --no-dependencies --no-restore --warnaserror --os win"
        );

        // https://github.com/moonrepo/moon/issues/1740
        assert_eq!(
            join_args(
                &create_bash(),
                [
                    "docker",
                    "build",
                    "--target",
                    "prod",
                    "-t",
                    "$project",
                    "-f",
                    "Dockerfile",
                    "$workspaceRoot",
                    "--build-arg",
                    "COMMIT_HASH=$(git rev-parse HEAD)"
                ]
            ),
            "docker build --target prod -t \"$project\" -f Dockerfile \"$workspaceRoot\" --build-arg \"COMMIT_HASH=$(git rev-parse HEAD)\""
        );
    }
}
