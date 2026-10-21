// SPDX-License-Identifier: MIT OR Apache-2.0
//! Finding the files a package links (bitmaps, vector art, fonts, lookup tables): by their path, then relative to
//! the package's folder and its `resources` folder, then by file name in those folders.
use std::path::{Path, PathBuf};
use std::sync::RwLock;

// ------------------------------------------------------------------------------------------------ file lookup
static SEARCH: RwLock<Vec<PathBuf>> = RwLock::new(Vec::new());

/// Folders searched for linked files that are given as relative paths (or whose absolute path no longer exists).
/// The editor sets this to the project folder; the command line sets it to the graph's folder.
pub fn set_search_dirs(dirs: Vec<PathBuf>) {
    if let Ok(mut w) = SEARCH.write() {
        *w = dirs;
    }
}

pub fn search_dirs() -> Vec<PathBuf> {
    SEARCH.read().map(|r| r.clone()).unwrap_or_default()
}

fn file_name_of(p: &str) -> &str {
    p.rsplit(['/', '\\']).next().unwrap_or(p)
}

/// Finds a linked file: the path itself (backslashes are accepted on every platform), then relative to each search
/// folder, then by file name inside each search folder.
pub fn resolve(path: &str) -> Option<PathBuf> {
    if path.is_empty() {
        return None;
    }
    let alt = path.replace('\\', "/");
    for c in [path, alt.as_str()] {
        let p = Path::new(c);
        if p.is_file() {
            return Some(p.to_path_buf());
        }
    }
    for d in search_dirs() {
        for c in [path, alt.as_str()] {
            let p = Path::new(c);
            if p.is_relative() {
                let j = d.join(p);
                if j.is_file() {
                    return Some(j);
                }
            }
        }
        let j = d.join(file_name_of(path));
        if j.is_file() {
            return Some(j);
        }
    }
    None
}

pub fn read_file(path: &str) -> Result<Vec<u8>, String> {
    match resolve(path) {
        Some(p) => std::fs::read(&p).map_err(|e| format!("cannot read '{path}': {e}")),
        None => Err(format!("cannot read '{path}': file not found")),
    }
}
