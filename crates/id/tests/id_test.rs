use starbase_id::Id;

mod id {
    use super::*;

    fn symbols() -> Vec<&'static str> {
        vec![".", "-", "_", "/"]
    }

    #[test]
    fn ascii() {
        for s in symbols() {
            assert!(Id::new(format!("abc{s}123")).is_ok());
        }

        assert!(Id::new("a.b-c_d/e").is_ok());
        assert!(Id::new("@a1").is_ok());
    }

    #[test]
    fn unicode() {
        for s in symbols() {
            assert!(Id::new(format!("ąęóąśłżźń{s}123")).is_ok());
        }

        assert!(Id::new("ą.ó-ą_ł/ń").is_ok());
        assert!(Id::new("@ż9").is_ok());
    }

    #[test]
    fn no_punc() {
        for p in ["'", "\"", "?", "?", "[", "}", "~", "`", "!", "@", "$"] {
            assert!(Id::new(format!("sbc{p}123")).is_err());
        }
    }

    #[test]
    fn doesnt_error_if_starts_with_a() {
        assert!(Id::new("@abc").is_ok());
    }

    #[test]
    fn errors_if_empty() {
        assert!(Id::new("").is_err());
    }

    #[test]
    fn to_env_var_uppercases_and_maps_dashes() {
        assert_eq!(Id::new("moon").unwrap().to_env_var(), "MOON");
        assert_eq!(Id::new("my-app").unwrap().to_env_var(), "MY_APP");
        // `@`, `/`, and `.` are dropped; dashes become underscores.
        assert_eq!(Id::new("@scope/pkg-1.2").unwrap().to_env_var(), "SCOPEPKG_12");
    }

    #[test]
    fn can_be_1_char() {
        assert!(Id::new("a").is_ok());
    }

    #[test]
    fn can_end_with_symbol() {
        for s in symbols() {
            assert!(Id::new(format!("abc{s}")).is_ok());
        }
    }

    #[test]
    fn supports_file_paths() {
        assert!(Id::new("packages/core/cli").is_ok());
    }

    #[test]
    fn supports_npm_package() {
        assert!(Id::new("@moonrepo/cli").is_ok());
    }
}
