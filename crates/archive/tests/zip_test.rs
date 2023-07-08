mod utils;

use starbase_archive::zip::*;
use starbase_archive::Archiver;
use starbase_sandbox::create_sandbox;
use std::path::Path;

mod zip {
    use super::*;

    generate_tests!("out.zip", |_, file| ZipPacker::new(file), ZipUnpacker::new);
}
