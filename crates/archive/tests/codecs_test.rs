use starbase_archive::codecs::*;
use std::io::{BufWriter, Cursor, Read, Write};

macro_rules! generate_codec_tests {
    ($name:ident, $codec:ident) => {
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

            #[test]
            fn roundtrips_with_custom_level() {
                let data = sample();

                let mut encoder = $codec::with_level(Vec::new(), 2);
                encoder.write_all(&data).unwrap();
                encoder.finish().unwrap();

                let compressed = encoder.into_inner().unwrap();

                let mut decoder = $codec::new(Cursor::new(compressed));
                let mut output = vec![];
                decoder.read_to_end(&mut output).unwrap();

                assert_eq!(output, data);
            }

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

generate_codec_tests!(bz2, Bz2);
generate_codec_tests!(gz, Gz);
generate_codec_tests!(xz, Xz);
generate_codec_tests!(zstd, Zstd);
