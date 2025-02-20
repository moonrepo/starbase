mod utils;

use starbase_archive::Archiver;
use starbase_archive::zip::*;
use starbase_sandbox::create_sandbox;
use std::path::Path;

mod zip {
    use super::*;

    generate_tests!("out.zip", ZipPacker::new, ZipUnpacker::new);
}

mod zip_deflate {
    use super::*;

    generate_tests!("out.zip", ZipPacker::new_deflate, ZipUnpacker::new_deflate);
}
