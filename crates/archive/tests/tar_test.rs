mod utils;

use starbase_archive::tar::*;
use starbase_archive::Archiver;
use starbase_sandbox::create_sandbox;
use std::path::Path;

mod tar_gz {
    use super::*;

    generate_tests!(
        "out.tar.gz",
        |_, file| TarPacker::<flate2::write::GzEncoder<std::fs::File>>::new_gz(file, None),
        TarUnpacker::<flate2::read::GzDecoder<std::fs::File>>::new_gz
    );
}

mod tar_xz {
    use super::*;

    generate_tests!(
        "out.tar.xz",
        |_, file| TarPacker::<xz2::write::XzEncoder<std::fs::File>>::new_xz(file, None),
        TarUnpacker::<xz2::read::XzDecoder<std::fs::File>>::new_xz
    );
}
