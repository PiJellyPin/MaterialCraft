// SPDX-License-Identifier: MIT OR Apache-2.0
//! Finding linked images, vector art, fonts and lookup tables by their original path,
//! package folders, then matching filenames in the application's assets folders.
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

/// The same assets locations used by the application's logo fallback. The working-directory
/// location also supports running a development build from the repository. Each
/// assets root is followed by its panorama folder for linked environment maps.
fn app_asset_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            dirs.push(parent.join("assets"));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        let assets = cwd.join("assets");
        if !dirs.contains(&assets) {
            dirs.push(assets);
        }
    }
    dirs.into_iter().flat_map(|root| [root.clone(), root.join("panorama")]).collect()
}

fn file_name_of(p: &str) -> &str {
    p.rsplit(['/', '\\']).next().unwrap_or(p)
}

/// Resolve with explicit package folders, independent of the editor's active package.
/// Existing paths win, followed by the supplied folders in order, then application assets.
/// Each folder is checked for the stored relative path and then the exact filename.
/// Application folders include assets and assets/panorama; other subfolders are not scanned.
/// Backslashes and file:// paths are accepted; other URI schemes (such as Adobe's
/// sd-3dview-maps://) can fall back to a matching filename without a network request.
pub fn resolve_in(path: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    let path = match path.strip_prefix("file://") {
        // file:///C:/... is a Windows drive path; file:///tmp/... must keep its leading slash.
        Some(p) if p.starts_with('/') && p.as_bytes().get(2) == Some(&b':') => &p[1..],
        Some(p) => p,
        None => path,
    };
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
    let assets = app_asset_dirs();
    for d in dirs.iter().chain(assets.iter()) {
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

/// Resolve through the active package's search folders and application assets.
pub fn resolve(path: &str) -> Option<PathBuf> {
    resolve_in(path, &search_dirs())
}

pub fn read_file(path: &str) -> Result<Vec<u8>, String> {
    match resolve(path) {
        Some(p) => std::fs::read(&p).map_err(|e| format!("cannot read '{path}': {e}")),
        None => Err(format!("cannot read '{path}': file not found")),
    }
}
