// SPDX-License-Identifier: MIT OR Apache-2.0
//! Linked SBS bitmap recovery uses generated pixels, never third-party artwork.
use materialcraft::{graph::Graph, png, resources, sbs_import, sbs_native, xmldoc::XDoc};
use std::path::{Path, PathBuf};

const BITMAP: u32 = 1584633001; // Resources/Untitled_Bitmap in all_nodes.sbs.
const COLOR: [f32; 4] = [0.2, 0.4, 0.6, 1.0];

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("mc_sbs_resources_{}_{stamp}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn picture(path: &Path, color: [f32; 4]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, png::encode(2, 2, &[color; 4], 8)).unwrap();
}

fn package(dir: &Path, stored_path: &str) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let mut doc = XDoc::parse(include_str!("assets/all_nodes.sbs")).expect("all_nodes.sbs parses");
    let resource = doc
        .root
        .path_mut(&["content", "group", "content"])
        .unwrap()
        .elems_mut()
        .find(|r| r.v("identifier").as_deref() == Some("Untitled_Bitmap"))
        .expect("all_nodes.sbs: bitmap resource");
    resource.child_mut("filepath").unwrap().set_attr("v", stored_path);
    let text = doc.to_text();
    let path = dir.join("example.sbs");
    std::fs::write(&path, text).unwrap();
    path
}

fn renders(g: &Graph, expected: [f32; 4], context: &str) {
    let t = g.evaluate_node(BITMAP, 16).unwrap_or_else(|e| panic!("{context}: all_nodes.sbs bitmap {BITMAP}: render failed: {e}"));
    assert_eq!((t.w, t.h), (16, 16), "{context}");
    assert!(t.px.iter().all(|p| p.iter().zip(expected).all(|(a, b)| (a - b).abs() <= 1.0 / 255.0)), "{context}: wrong image, first pixel {:?}", t.px[0]);
}

fn imported_and_native(path: &Path, expected_file: &Path, color: [f32; 4]) {
    let source = std::fs::read_to_string(path).unwrap();
    let imported = sbs_import::import_file(path).unwrap();
    assert!(!imported.report.notes.iter().any(|n| n.text.contains("Untitled_Bitmap")), "{}: unexpected diagnostics:\n{}", path.display(), imported.report.to_text());
    assert_eq!(imported.graph.nodes[&BITMAP].strs.get("file").map(PathBuf::from), Some(expected_file.to_path_buf()), "{}: wrong resolved image", path.display());
    renders(&imported.graph, color, "direct import");
    let (native, warnings) = sbs_native::open(path).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let lowered = sbs_native::lower(&native).unwrap();
    assert!(!lowered.report.notes.iter().any(|n| n.text.contains("Untitled_Bitmap")), "native conversion:\n{}", lowered.report.to_text());
    renders(&native, color, "native rendering");
    let saved = sbs_native::save_text(&native).unwrap();
    assert!(saved == source, "recovering an image must not rewrite all_nodes.sbs or its stored paths");
}

#[test]
fn stale_bitmap_paths_resolve_in_package_folders_without_global_setup() {
    let temp = TempDir::new();
    for (i, stored) in
        ["/missing/old/location/sample.png", r"C:\missing\old\location\sample.png", "sd-3dview-maps://sample.png", "file:///missing/old/location/sample.png"].iter().enumerate()
    {
        for (j, folder) in ["", "resources", "example.resources"].iter().enumerate() {
            let dir = temp.0.join(format!("case_{i}_{j}"));
            let file = dir.join(folder).join("sample.png");
            picture(&file, COLOR);
            let path = package(&dir, stored);
            imported_and_native(&path, &file, COLOR);
        }
    }
}

#[test]
fn original_bitmap_paths_win_over_filename_fallbacks() {
    let temp = TempDir::new();
    let dir = temp.0.join("project");
    let actual = dir.join("original/sample.png");
    picture(&actual, COLOR);
    picture(&dir.join("sample.png"), [1.0, 0.0, 0.0, 1.0]);
    let absolute = actual.to_string_lossy().replace('\\', "/");
    let file_uri = if absolute.starts_with('/') { format!("file://{absolute}") } else { format!("file:///{absolute}") };
    for stored in [absolute, file_uri, "original/sample.png".into(), r"original\sample.png".into()] {
        let path = package(&dir, &stored);
        imported_and_native(&path, &actual, COLOR);
    }
}

#[test]
fn missing_bitmap_diagnostics_name_the_resource_and_search_locations() {
    let temp = TempDir::new();
    let stored = "sd-3dview-maps://mc_missing_resource_7da22a.png";
    let path = package(&temp.0, stored);
    let imported = sbs_import::import_file(&path).unwrap();
    let report = imported.report.to_text();
    assert!(report.contains(stored) && report.contains("Untitled_Bitmap") && report.contains("application assets") && report.contains("assets/panorama"), "{report}");
    assert!(report.contains(&temp.0.display().to_string()), "missing diagnostic must show where package files were searched:\n{report}");
    assert!(imported.graph.evaluate_node(BITMAP, 16).unwrap_err().to_string().contains("file not found"));
}

#[test]
fn application_assets_recover_images_and_package_files_take_priority() {
    // A child process gives this test its own executable/assets and working-directory/assets
    // without racing other tests by changing the process-wide working directory or search folders.
    const CHILD: &str = "MC_SBS_RESOURCE_LOOKUP_TEST_CHILD";
    const FILES: [&str; 4] = ["app/assets/exe_image.png", "working/assets/cwd_image.png", "app/assets/panorama/exe_panorama.png", "working/assets/panorama/cwd_panorama.png"];
    if let Some(root) = std::env::var_os(CHILD).map(PathBuf::from) {
        for relative in FILES {
            let file = root.join(relative);
            let name = file.file_name().unwrap().to_str().unwrap();
            let stored = format!("sd-3dview-maps://{name}");
            let path = package(&root.join("project"), &stored);
            assert_eq!(resources::resolve(&stored), Some(file.clone()), "renderer must search application assets");
            imported_and_native(&path, &file, COLOR);

            let local = root.join("project").join(name);
            picture(&local, [0.8, 0.2, 0.4, 1.0]);
            imported_and_native(&path, &local, [0.8, 0.2, 0.4, 1.0]);
            resources::set_search_dirs(vec![root.join("project")]);
            assert_eq!(resources::resolve(&stored), Some(local), "package-local images must win over app assets");
            resources::set_search_dirs(vec![]);
        }
        return;
    }

    let temp = TempDir::new();
    for relative in FILES {
        picture(&temp.0.join(relative), COLOR);
    }
    // Direct assets win over panorama fallback within the same installation.
    picture(&temp.0.join("app/assets/panorama/exe_image.png"), [1.0, 0.0, 0.0, 1.0]);
    picture(&temp.0.join("working/assets/exe_panorama.png"), [1.0, 0.0, 0.0, 1.0]);
    // An identically named fallback in the working directory must not override executable assets.
    picture(&temp.0.join("working/assets/exe_image.png"), [1.0, 0.0, 0.0, 1.0]);
    let exe = temp.0.join("app").join(format!("resource_test{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(std::env::current_exe().unwrap(), &exe).unwrap();
    let result = std::process::Command::new(exe)
        .args(["--exact", "application_assets_recover_images_and_package_files_take_priority", "--nocapture"])
        .current_dir(temp.0.join("working"))
        .env(CHILD, &temp.0)
        .output()
        .unwrap();
    assert!(result.status.success(), "isolated application-assets lookup failed:\n{}\n{}", String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
}
