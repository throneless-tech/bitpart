use std::ffi::OsStr;
use std::{fs, io, path::PathBuf};

fn find_csml(path: &str) -> io::Result<Vec<PathBuf>> {
    let entries = fs::read_dir(path)?
        .filter_map(|res| match res.ok()?.path() {
            path if path.extension() == Some(OsStr::new("csml")) => Some(path),
            _ => None,
        })
        .collect::<Vec<_>>();

    Ok(entries)
}
