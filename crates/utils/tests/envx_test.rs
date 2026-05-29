use starbase_utils::envx;

fn with_env_var(name: &str, value: &str, test: impl FnOnce()) {
    unsafe {
        std::env::set_var(name, value);
    }

    test();

    unsafe {
        std::env::remove_var(name);
    }
}

mod bool_var {
    use super::*;

    #[test]
    fn accepts_truthy_values_case_insensitively() {
        for (index, value) in ["1", "TRUE", "Yes", "oN", "Enable"].into_iter().enumerate() {
            let name = format!("STARBASE_UTILS_BOOL_VAR_TRUTHY_{index}");

            with_env_var(&name, value, || {
                assert!(envx::bool_var(&name));
            });
        }
    }

    #[test]
    fn rejects_falsey_and_missing_values() {
        with_env_var("STARBASE_UTILS_BOOL_VAR_FALSEY", "false", || {
            assert!(!envx::bool_var("STARBASE_UTILS_BOOL_VAR_FALSEY"));
        });

        assert!(!envx::bool_var("STARBASE_UTILS_BOOL_VAR_MISSING"));
    }
}
