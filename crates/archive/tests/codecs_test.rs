use starbase_archive::codecs::*;
use std::io::{BufWriter, Cursor, Read, Write};

macro_rules! generate_codec_tests {
    ($name:ident, $codec:ident $(, with_level = $level:literal)?) => {
        mod $name {
            use super::*;

            fn sample() -> Vec<u8> {
                b"starbase archive codec roundtrip ".repeat(1024)
            }

            #[test]
            fn roundtrips() {
                let data = sample();

                let mut encoder = $codec::new(Vec::new());
                encoder.write_all(&data).unwrap();
                encoder.finish().unwrap();

                let compressed = encoder.into_inner().unwrap();

                assert!(!compressed.is_empty());
                assert_ne!(compressed, data);
                assert!(compressed.len() < data.len());

                let mut decoder = $codec::new(Cursor::new(compressed));
                let mut output = vec![];
                decoder.read_to_end(&mut output).unwrap();

                assert_eq!(output, data);
            }

            $(
            #[test]
            fn roundtrips_with_custom_level() {
                let data = sample();

                let mut encoder = $codec::with_level(Vec::new(), $level);
                encoder.write_all(&data).unwrap();
                encoder.finish().unwrap();

                let compressed = encoder.into_inner().unwrap();

                let mut decoder = $codec::new(Cursor::new(compressed));
                let mut output = vec![];
                decoder.read_to_end(&mut output).unwrap();

                assert_eq!(output, data);
            }
            )?

            #[test]
            fn roundtrips_through_buf_writer() {
                let data = sample();

                let mut encoder = $codec::new(BufWriter::new(Vec::new()));
                encoder.write_all(&data).unwrap();
                encoder.finish().unwrap();

                let compressed = encoder
                    .into_inner()
                    .unwrap()
                    .into_inner()
                    .expect("buffer flushed by finish");

                let mut decoder = $codec::new(Cursor::new(compressed));
                let mut output = vec![];
                decoder.read_to_end(&mut output).unwrap();

                assert_eq!(output, data);
            }

            #[test]
            fn finish_is_idempotent() {
                let mut encoder = $codec::new(Vec::new());
                encoder.write_all(b"data").unwrap();
                encoder.finish().unwrap();
                encoder.finish().unwrap();
            }

            #[test]
            fn finish_without_writes_creates_valid_empty_stream() {
                let mut encoder = $codec::new(Vec::new());
                encoder.finish().unwrap();

                let compressed = encoder.into_inner().unwrap();

                assert!(!compressed.is_empty());

                let mut decoder = $codec::new(Cursor::new(compressed));
                let mut output = vec![];
                decoder.read_to_end(&mut output).unwrap();

                assert!(output.is_empty());
            }

            #[test]
            fn into_inner_writes_epilogue_without_explicit_finish() {
                let data = sample();

                let mut encoder = $codec::new(Vec::new());
                encoder.write_all(&data).unwrap();

                // No finish() call; into_inner() must complete the stream.
                let compressed = encoder.into_inner().unwrap();

                let mut decoder = $codec::new(Cursor::new(compressed));
                let mut output = vec![];
                decoder.read_to_end(&mut output).unwrap();

                assert_eq!(output, data);
            }

            #[test]
            fn errors_reading_after_writing() {
                let mut codec = $codec::new(Cursor::new(Vec::new()));
                codec.write_all(b"data").unwrap();

                let mut buf = [0u8; 16];

                assert!(codec.read(&mut buf).is_err());
            }

            #[test]
            fn errors_writing_after_reading() {
                let mut encoder = $codec::new(Vec::new());
                encoder.write_all(b"data").unwrap();
                encoder.finish().unwrap();

                let compressed = encoder.into_inner().unwrap();

                let mut codec = $codec::new(Cursor::new(compressed));
                let mut output = vec![];
                codec.read_to_end(&mut output).unwrap();

                assert!(codec.write(b"nope").is_err());
            }

            #[test]
            fn errors_writing_after_finish() {
                let mut encoder = $codec::new(Vec::new());
                encoder.write_all(b"data").unwrap();
                encoder.finish().unwrap();

                assert!(encoder.write(b"more").is_err());
            }
        }
    };
}

generate_codec_tests!(bz2, Bz2, with_level = 2);
generate_codec_tests!(gz, Gz, with_level = 2);
generate_codec_tests!(xz, Xz, with_level = 2);
generate_codec_tests!(z, Z);
generate_codec_tests!(zstd, Zstd, with_level = 2);

/// The `.Z` codec is implemented natively, so pin it against reference
/// vectors and output captured from the system `compress` tool.
mod z_reference {
    use super::*;

    /// Deterministic incompressible bytes, which overflow the 16-bit
    /// string table after roughly 130 KB and force the clear path on
    /// both sides.
    fn pseudo_random(len: usize) -> Vec<u8> {
        let mut state: u64 = 0x9E3779B97F4A7C15;
        let mut out = Vec::with_capacity(len + 8);

        while out.len() < len {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            out.extend_from_slice(&state.to_le_bytes());
        }

        out.truncate(len);
        out
    }

    fn decode(bytes: &[u8]) -> std::io::Result<Vec<u8>> {
        let mut decoder = Z::new(Cursor::new(bytes.to_vec()));
        let mut output = vec![];
        decoder.read_to_end(&mut output)?;

        Ok(output)
    }

    #[test]
    fn roundtrips_data_larger_than_the_string_table() {
        let data = pseudo_random(500_000);

        let mut encoder = Z::new(Vec::new());
        encoder.write_all(&data).unwrap();
        encoder.finish().unwrap();

        assert_eq!(decode(&encoder.into_inner().unwrap()).unwrap(), data);
    }

    #[test]
    fn roundtrips_single_repeated_byte() {
        // Runs produce codes that reference the entry being defined
        // (the KwKwK special case)
        let data = vec![b'a'; 100_000];

        let mut encoder = Z::new(Vec::new());
        encoder.write_all(&data).unwrap();
        encoder.finish().unwrap();

        assert_eq!(decode(&encoder.into_inner().unwrap()).unwrap(), data);
    }

    #[test]
    fn matches_system_compress_output() {
        let data = include_bytes!("__fixtures__/archives/file.txt");
        let expected = include_bytes!("__fixtures__/compress/file.txt.Z");

        let mut encoder = Z::new(Vec::new());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap();

        // Short streams never clear the table, so output is bit-exact
        assert_eq!(encoder.into_inner().unwrap(), expected);
        assert_eq!(decode(expected).unwrap(), data);
    }

    #[test]
    fn decodes_non_block_mode_streams() {
        // Hand-packed pre-1986 style stream (header flags 0x10: 16-bit
        // codes, no reserved clear code) holding "ababababab"
        let stream = [0x1F, 0x9D, 0x10, 0x61, 0xC4, 0x00, 0x14, 0x18, 0x50, 0x0C];

        assert_eq!(decode(&stream).unwrap(), b"ababababab");
    }

    #[test]
    fn errors_on_bad_magic() {
        let error = decode(b"\x1F\x8B\x90whatever").unwrap_err();

        assert!(error.to_string().contains("bad magic"), "{error}");
    }

    #[test]
    fn errors_on_truncated_header() {
        for stream in [&b""[..], &b"\x1F"[..], &b"\x1F\x9D"[..]] {
            let error = decode(stream).unwrap_err();

            assert!(error.to_string().contains("header"), "{error}");
        }
    }

    #[test]
    fn errors_when_first_code_is_not_a_literal() {
        // Header, then a single 9-bit code of 300
        let error = decode(&[0x1F, 0x9D, 0x90, 0x2C, 0x01]).unwrap_err();

        assert!(error.to_string().contains("literal"), "{error}");
    }

    #[test]
    fn errors_on_code_beyond_the_string_table() {
        // Header, then 9-bit codes 97 and 400; only 257 entries exist
        let error = decode(&[0x1F, 0x9D, 0x90, 0x61, 0x20, 0x03]).unwrap_err();

        assert!(error.to_string().contains("string table"), "{error}");
    }
}
