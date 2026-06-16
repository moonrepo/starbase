use crate::fs;
use std::fmt::Debug;
use std::io::{self, Read};
use std::path::Path;
use tracing::instrument;

pub use crate::hash_error::HashError;
pub use hex;

#[cfg(feature = "hash-base64")]
/// Generate Base64 encoded hashes.
pub mod base64 {
    use super::*;
    pub use ::base64 as native;
    use ::base64::{prelude::*, write::EncoderWriter};

    /// Create a hash based on the provided value.
    #[inline]
    #[instrument(skip(value))]
    pub fn from_bytes<T: AsRef<[u8]>>(value: T) -> String {
        BASE64_STANDARD.encode(value)
    }

    /// Create a hash based on the provided file path.
    #[inline]
    #[instrument]
    pub fn from_file<P: AsRef<Path> + Debug>(path: P) -> Result<String, HashError> {
        from_reader(fs::open_file(path)?)
    }

    /// Create a hash based on the provided reader.
    #[inline]
    #[instrument(skip(reader))]
    pub fn from_reader<R: Read>(mut reader: R) -> Result<String, HashError> {
        let mut output = Vec::new();

        {
            let mut encoder = EncoderWriter::new(&mut output, &BASE64_STANDARD);

            io::copy(&mut reader, &mut encoder).map_err(|error| HashError::ReadStream {
                error: Box::new(error),
            })?;

            // Flush remaining buffered bytes before `output` is read; the `Drop`
            // impl does this too, but silently ignores any error.
            encoder.finish().map_err(|error| HashError::ReadStream {
                error: Box::new(error),
            })?;
        }

        // SAFETY: Base64 output is restricted to the ASCII alphabet, so it is
        // always valid UTF-8.
        Ok(unsafe { String::from_utf8_unchecked(output) })
    }
}

macro_rules! generate_sha_funcs {
    ($name:ident, $digest:ident) => {
        #[doc = concat!("Generate ", stringify!($digest), " based hashes.")]
        pub mod $name {
            use super::*;
            pub use sha2 as native;
            use sha2::{Digest, $digest};

            /// Create a hash based on the provided value.
            #[inline]
            #[instrument(skip(value))]
            pub fn from_bytes<T: AsRef<[u8]>>(value: T) -> String {
                hex::encode($digest::digest(value))
            }

            /// Create a hash based on the provided file path.
            #[inline]
            #[instrument]
            pub fn from_file<P: AsRef<Path> + Debug>(path: P) -> Result<String, HashError> {
                from_reader(fs::open_file(path)?)
            }

            /// Create a hash based on the provided reader.
            #[inline]
            #[instrument(skip(reader))]
            pub fn from_reader<R: Read>(reader: R) -> Result<String, HashError> {
                hash_sha_reader(reader, $digest::new())
            }
        }
    };
}

fn hash_sha_reader<R: Read, D: sha2::Digest>(
    mut reader: R,
    mut sha: D,
) -> Result<String, HashError> {
    let mut buffer = [0u8; 64 * 1024];

    // Read in chunks instead of pulling the entire file into memory
    loop {
        let n = reader
            .read(&mut buffer)
            .map_err(|error| HashError::ReadStream {
                error: Box::new(error),
            })?;

        if n == 0 {
            break;
        }

        sha.update(&buffer[..n]);
    }

    Ok(hex::encode(sha.finalize()))
}

generate_sha_funcs!(sha256, Sha256);
generate_sha_funcs!(sha512, Sha512);
