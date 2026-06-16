use starbase_sandbox::create_empty_sandbox;
use starbase_utils::hash::{base64, sha256, sha512};
use std::io::Cursor;

// All known-answer vectors below were verified against the `shasum` and
// `base64` CLIs. Inputs intentionally include lengths that are not multiples
// of 3 (base64 encodes in 3-byte groups) and one input larger than the 64 KB
// read buffer used by the SHA reader.

mod base64_hash {
    use super::*;

    #[test]
    fn from_bytes_known_vectors() {
        assert_eq!(base64::from_bytes(""), "");
        assert_eq!(base64::from_bytes("a"), "YQ==");
        assert_eq!(base64::from_bytes("ab"), "YWI=");
        assert_eq!(base64::from_bytes("abc"), "YWJj");
        assert_eq!(base64::from_bytes("hello"), "aGVsbG8=");
        assert_eq!(base64::from_bytes("hello world"), "aGVsbG8gd29ybGQ=");
    }

    #[test]
    fn from_reader_matches_bytes() {
        for input in ["", "a", "ab", "abc", "hello", "hello world"] {
            assert_eq!(
                base64::from_reader(Cursor::new(input)).unwrap(),
                base64::from_bytes(input),
                "mismatch for {input:?}"
            );
        }
    }

    #[test]
    fn from_reader_handles_large_input() {
        let input = "abcdefghij".repeat(20_000); // 200 KB
        assert_eq!(
            base64::from_reader(Cursor::new(&input)).unwrap(),
            base64::from_bytes(&input)
        );
    }

    #[test]
    fn from_file_matches_bytes() {
        let sandbox = create_empty_sandbox();
        sandbox.create_file("data.txt", "hello world");

        assert_eq!(
            base64::from_file(sandbox.path().join("data.txt")).unwrap(),
            base64::from_bytes("hello world")
        );
    }

    #[test]
    fn from_file_errors_for_missing_path() {
        let sandbox = create_empty_sandbox();

        assert!(base64::from_file(sandbox.path().join("missing.txt")).is_err());
    }
}

mod sha256_hash {
    use super::*;

    #[test]
    fn from_bytes_known_vectors() {
        assert_eq!(
            sha256::from_bytes("").unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256::from_bytes("abc").unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256::from_bytes("hello").unwrap(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        assert_eq!(
            sha256::from_bytes("hello world").unwrap(),
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn from_reader_matches_bytes() {
        for input in ["", "abc", "hello", "hello world"] {
            assert_eq!(
                sha256::from_reader(Cursor::new(input)).unwrap(),
                sha256::from_bytes(input).unwrap(),
                "mismatch for {input:?}"
            );
        }
    }

    #[test]
    fn from_reader_handles_large_input() {
        // Larger than the 64 KB read buffer, so the chunked loop runs repeatedly.
        let input = "abcdefghij".repeat(20_000); // 200 KB
        assert_eq!(
            sha256::from_reader(Cursor::new(&input)).unwrap(),
            sha256::from_bytes(&input).unwrap()
        );
    }

    #[test]
    fn from_file_matches_bytes() {
        let sandbox = create_empty_sandbox();
        sandbox.create_file("data.txt", "hello world");

        assert_eq!(
            sha256::from_file(sandbox.path().join("data.txt")).unwrap(),
            sha256::from_bytes("hello world").unwrap()
        );
    }

    #[test]
    fn from_file_errors_for_missing_path() {
        let sandbox = create_empty_sandbox();

        assert!(sha256::from_file(sandbox.path().join("missing.txt")).is_err());
    }
}

mod sha512_hash {
    use super::*;

    #[test]
    fn from_bytes_known_vectors() {
        assert_eq!(
            sha512::from_bytes("").unwrap(),
            "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce\
             47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e"
        );
        assert_eq!(
            sha512::from_bytes("abc").unwrap(),
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a\
             2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
        );
        assert_eq!(
            sha512::from_bytes("hello").unwrap(),
            "9b71d224bd62f3785d96d46ad3ea3d73319bfbc2890caadae2dff72519673ca7\
             2323c3d99ba5c11d7c7acc6e14b8c5da0c4663475c2e5c3adef46f73bcdec043"
        );
    }

    #[test]
    fn from_reader_matches_bytes() {
        for input in ["", "abc", "hello", "hello world"] {
            assert_eq!(
                sha512::from_reader(Cursor::new(input)).unwrap(),
                sha512::from_bytes(input).unwrap(),
                "mismatch for {input:?}"
            );
        }
    }

    #[test]
    fn from_reader_handles_large_input() {
        let input = "abcdefghij".repeat(20_000); // 200 KB
        assert_eq!(
            sha512::from_reader(Cursor::new(&input)).unwrap(),
            sha512::from_bytes(&input).unwrap()
        );
    }

    #[test]
    fn from_file_matches_bytes() {
        let sandbox = create_empty_sandbox();
        sandbox.create_file("data.txt", "hello world");

        assert_eq!(
            sha512::from_file(sandbox.path().join("data.txt")).unwrap(),
            sha512::from_bytes("hello world").unwrap()
        );
    }

    #[test]
    fn from_file_errors_for_missing_path() {
        let sandbox = create_empty_sandbox();

        assert!(sha512::from_file(sandbox.path().join("missing.txt")).is_err());
    }
}
