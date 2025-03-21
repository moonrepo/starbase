mod utils;

use starbase_archive::Archiver;
use starbase_archive::tar::*;
use starbase_sandbox::create_sandbox;
use std::path::Path;

mod tar {
    use super::*;

    generate_tests!("out.tar", TarPacker::new, TarUnpacker::new);
}

mod tar_gz {
    use super::*;

    generate_tests!("out.tar.gz", TarPacker::new_gz, TarUnpacker::new_gz);
}

mod tar_xz {
    use super::*;

    generate_tests!("out.tar.xz", TarPacker::new_xz, TarUnpacker::new_xz);
}

mod tar_zstd {
    use super::*;

    generate_tests!("out.tar.zst", TarPacker::new_zstd, TarUnpacker::new_zstd);
}

mod tar_bz2 {
    use super::*;

    generate_tests!("out.tar.bz2", TarPacker::new_bz2, TarUnpacker::new_bz2);
}
