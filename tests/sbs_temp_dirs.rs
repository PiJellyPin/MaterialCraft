// SPDX-License-Identifier: MIT OR Apache-2.0
//! Regression for colliding temporary-directory names in parallel SBS tests.
mod common;
use common::TempDir;
use std::collections::BTreeSet;
use std::sync::Barrier;

#[test]
fn temporary_directories_skip_existing_paths_and_remain_isolated() {
    // This integration binary has one test, so the allocator's first candidate is zero.
    // Occupy it before creating directories; only clean it up if this test created it.
    let occupied_path = std::env::temp_dir().join(format!("materialcraft_sbs_{}_0", std::process::id()));
    let occupied = match std::fs::create_dir(&occupied_path) {
        Ok(()) => Some(TempDir(occupied_path.clone())),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => None,
        Err(e) => panic!("create {}: {e}", occupied_path.display()),
    };
    if let Some(dir) = &occupied {
        dir.write("owner.txt", "existing directory");
    }

    const WORKERS: usize = 16;
    const PER_WORKER: usize = 32;
    let start = Barrier::new(WORKERS);
    let dirs: Vec<(TempDir, String)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..WORKERS).map(|worker| {
            let start = &start;
            scope.spawn(move || {
                start.wait();
                (0..PER_WORKER).map(|i| {
                    let dir = TempDir::new();
                    let owner = format!("worker {worker}, directory {i}");
                    dir.write("owner.txt", &owner);
                    (dir, owner)
                }).collect::<Vec<_>>()
            })
        }).collect();
        workers.into_iter().flat_map(|worker| worker.join().unwrap()).collect()
    });
    let paths: BTreeSet<_> = dirs.iter().map(|(dir, _)| dir.0.clone()).collect();
    assert_eq!(paths.len(), WORKERS * PER_WORKER, "each allocation must own a distinct directory");
    assert!(!paths.contains(&occupied_path), "an existing directory must never be reused");
    for (dir, owner) in &dirs {
        assert_eq!(std::fs::read_to_string(dir.0.join("owner.txt")).unwrap(), *owner);
    }
    drop(dirs);
    assert!(paths.iter().all(|p| !p.exists()), "owned directories must be removed on drop");
    assert!(std::fs::symlink_metadata(&occupied_path).is_ok(), "cleanup must preserve an existing path");
    if let Some(dir) = &occupied {
        assert_eq!(std::fs::read_to_string(dir.0.join("owner.txt")).unwrap(), "existing directory");
    }
}
