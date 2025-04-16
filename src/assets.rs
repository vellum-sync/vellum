use std::path::Path;

use include_dir::{Dir, File, include_dir};

static ASSETS: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/assets/");

pub fn get_file<P: AsRef<Path>>(path: P) -> Option<&'static File<'static>> {
    ASSETS.get_file(path)
}
