// SPDX-License-Identifier: MIT OR Apache-2.0
//! MaterialX export: the document is well formed, every reference resolves, the files are written the way Unreal
//! Engine 5.8's importer reads them. `tests/tools/validate_mtlx.py` checks the same files with the official
//! MaterialX 1.39 library (run by hand or in CI where Python and `pip install MaterialX` exist).
use materialcraft::mtlx::{self, Map, NormalY, Options, Shader};
use materialcraft::xml::{parse, Element};
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::path::PathBuf;

fn all_maps() -> BTreeMap<Map, String> {
    [Map::BaseColor, Map::Roughness, Map::Metallic, Map::Normal, Map::Height, Map::AmbientOcclusion, Map::Opacity, Map::Emissive]
        .iter()
        .map(|m| (*m, format!("textures/m_{}.png", m.suffix())))
        .collect()
}

fn walk<'a>(e: &'a Element, f: &mut dyn FnMut(&'a Element)) {
    f(e);
    for c in &e.children {
        walk(c, f);
    }
}

/// Every nodename / nodegraph / output reference points at something that exists, names are unique per scope.
fn check_references(root: &Element) {
    assert_eq!(root.name, "materialx");
    assert_eq!(root.attr("version"), Some("1.39"));
    let top: HashSet<&str> = root.children.iter().filter_map(|c| c.attr("name")).collect();
    assert_eq!(top.len(), root.children.iter().filter(|c| c.attr("name").is_some()).count(), "duplicate top-level names");
    for ng in root.children.iter().filter(|c| c.name == "nodegraph") {
        let inside: Vec<&str> = ng.children.iter().filter(|c| c.name != "output").filter_map(|c| c.attr("name")).collect();
        let set: HashSet<&&str> = inside.iter().collect();
        assert_eq!(set.len(), inside.len(), "duplicate names in a node graph");
        for n in &ng.children {
            for i in n.children.iter().filter(|c| c.name == "input") {
                if let Some(r) = i.attr("nodename") {
                    assert!(inside.contains(&r), "{} refers to missing node {r}", n.attr("name").unwrap_or("?"));
                }
            }
            if n.name == "output" {
                let r = n.attr("nodename").expect("output without node");
                assert!(inside.contains(&r), "output refers to missing node {r}");
            }
        }
    }
    let outputs = |g: &str| -> HashSet<String> { root.children.iter().filter(|c| c.name == "nodegraph" && c.attr("name") == Some(g)).flat_map(|c| c.children.iter().filter(|o| o.name == "output").filter_map(|o| o.attr("name").map(String::from))).collect() };
    for n in root.children.iter().filter(|c| c.name != "nodegraph") {
        for i in n.children.iter().filter(|c| c.name == "input") {
            if let Some(g) = i.attr("nodegraph") {
                let o = i.attr("output").expect("nodegraph input without output");
                assert!(outputs(g).contains(o), "{g} has no output {o}");
            }
            if let Some(r) = i.attr("nodename") {
                assert!(top.contains(r), "missing node {r}");
            }
        }
    }
}

#[test]
fn standard_surface_document_is_complete_and_consistent() {
    let o = Options::default();
    let text = mtlx::document("Brick Wall", &all_maps(), &o);
    let root = parse(&text).expect("well formed");
    check_references(&root);
    let mut files = 0;
    walk(&root, &mut |e| {
        if e.name == "input" && e.attr("name") == Some("file") {
            files += 1;
            let v = e.attr("value").unwrap();
            assert!(v.starts_with("textures/") && !v.contains(".."), "{v}");
            let colour = v.contains("basecolor") || v.contains("emissive");
            // Unreal decides sRGB from this exact name
            assert_eq!(e.attr("colorspace") == Some("srgb_texture"), colour, "{v}");
        }
    });
    assert_eq!(files, 8);
    let ss = root.children.iter().find(|c| c.name == "standard_surface").expect("standard_surface");
    for want in ["base_color", "specular_roughness", "metalness", "normal", "emission", "emission_color", "opacity"] {
        assert!(ss.children.iter().any(|i| i.attr("name") == Some(want)), "missing {want}");
    }
    let mat = root.children.iter().find(|c| c.name == "surfacematerial").expect("surfacematerial");
    assert!(mat.children.iter().any(|i| i.attr("name") == Some("displacementshader")));
    assert_eq!(mat.attr("name"), Some("M_Brick_Wall"));
}

#[test]
fn openpbr_document_uses_its_own_input_names() {
    let o = Options { shader: Shader::OpenPbr, ..Options::default() };
    let text = mtlx::document("m", &all_maps(), &o);
    let root = parse(&text).unwrap();
    check_references(&root);
    let s = root.children.iter().find(|c| c.name == "open_pbr_surface").expect("open_pbr_surface");
    for want in ["base_color", "specular_roughness", "base_metalness", "geometry_normal", "emission_color", "geometry_opacity"] {
        assert!(s.children.iter().any(|i| i.attr("name") == Some(want)), "missing {want}");
    }
    // opacity is a float there: no conversion node
    assert!(!text.contains("opacity_color"));
}

#[test]
fn missing_maps_leave_their_inputs_out() {
    let mut m = BTreeMap::new();
    m.insert(Map::BaseColor, "textures/a.png".to_string());
    let text = mtlx::document("only_color", &m, &Options::default());
    let root = parse(&text).unwrap();
    check_references(&root);
    assert!(!text.contains("normalmap") && !text.contains("displacement"));
    assert!(text.contains("<input name=\"metalness\" type=\"float\" value=\"0.0\""));
    let no_disp = mtlx::document("x", &all_maps(), &Options { displacement: false, ..Options::default() });
    assert!(!no_disp.contains("<displacement"), "displacement node must follow the option");
}

#[test]
fn names_and_paths_are_safe() {
    assert_eq!(mtlx::safe_name("Brick Wall (2K)"), "Brick_Wall_2K");
    assert_eq!(mtlx::safe_name("1st floor"), "M_1st_floor");
    assert_eq!(mtlx::safe_name("../..//x"), "x");
    assert_eq!(mtlx::safe_name("   "), "Material");
    let text = mtlx::document("a\"b<c>&", &BTreeMap::new(), &Options::default());
    assert!(parse(&text).is_ok(), "a hostile name must not break the XML");
    // a texture file name with markup characters is escaped
    let mut m = BTreeMap::new();
    m.insert(Map::BaseColor, "textures/a&b\".png".to_string());
    assert!(parse(&mtlx::document("n", &m, &Options::default())).is_ok());
}

#[test]
fn exporting_a_package_writes_the_files_the_document_names() {
    let dir = std::env::temp_dir().join(format!("mc_mtlx_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/templates/brick_wall.sbs");
    let (g, _) = materialcraft::sbs_native::open(&path).expect("template opens");
    let o = Options { size: Some(64), ..Options::default() };
    let r = mtlx::export(&g, &dir, &o).expect("export");
    assert!(r.mtlx.is_file());
    let text = std::fs::read_to_string(&r.mtlx).unwrap();
    let root = parse(&text).unwrap();
    check_references(&root);
    let mut n = 0;
    walk(&root, &mut |e| {
        if e.name == "input" && e.attr("name") == Some("file") {
            n += 1;
            let f = dir.join(e.attr("value").unwrap());
            let bytes = std::fs::read(&f).unwrap_or_else(|_| panic!("{} was not written", f.display()));
            assert_eq!(&bytes[1..4], b"PNG");
            // 64 x 64 as asked
            assert_eq!(u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]), 64);
        }
    });
    assert!(n >= 3, "a material needs at least colour, roughness and normal");
    assert_eq!(r.files.len(), n);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn normal_direction_follows_the_option() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/templates/brick_wall.sbs");
    let (g, _) = materialcraft::sbs_native::open(&path).unwrap();
    let out = g.evaluate(Some(64)).unwrap();
    let mut w = vec![];
    let dx = mtlx::collect(&g, &out, &Options { normal: NormalY::DirectX, ..Options::default() }, &mut w);
    let gl = mtlx::collect(&g, &out, &Options { normal: NormalY::OpenGl, ..Options::default() }, &mut w);
    let (a, b) = (&dx[&Map::Normal], &gl[&Map::Normal]);
    for (p, q) in a.px.iter().zip(&b.px).step_by(37) {
        assert!((p[0] - q[0]).abs() < 1e-6 && (p[2] - q[2]).abs() < 1e-6, "red and blue must not change");
        assert!((p[1] + q[1] - 1.0).abs() < 1e-5, "green is mirrored");
    }
}

#[test]
fn bad_options_are_errors() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/templates/brick_wall.sbs");
    let (g, _) = materialcraft::sbs_native::open(&path).unwrap();
    let d = std::env::temp_dir().join("mc_mtlx_bad");
    assert!(mtlx::export(&g, &d, &Options { depth: 12, ..Options::default() }).is_err());
    assert!(mtlx::export(&g, &d, &Options { size: Some(4), ..Options::default() }).is_err());
    assert!(mtlx::export(&g, &d, &Options { texture_dir: "../x".into(), ..Options::default() }).is_err());
    assert!(mtlx::export(&g, &d, &Options { texture_dir: String::new(), ..Options::default() }).is_err());
}

/// With MC_MTLX_DUMP=<dir> the full documents (every map, both shaders) are written there with stand-in textures, for
/// `tests/tools/validate_mtlx.py`.
#[test]
fn dump_documents_for_the_external_validator() {
    let Ok(dir) = std::env::var("MC_MTLX_DUMP") else { return };
    let dir = PathBuf::from(dir);
    std::fs::create_dir_all(dir.join("textures")).unwrap();
    for m in all_maps().values() {
        std::fs::write(dir.join(m), b"x").unwrap();
    }
    for (n, sh) in [("full_standard", Shader::StandardSurface), ("full_openpbr", Shader::OpenPbr)] {
        let text = mtlx::document(n, &all_maps(), &Options { shader: sh, tiling: [2.0, 3.5], ..Options::default() });
        std::fs::write(dir.join(format!("{n}.mtlx")), text).unwrap();
    }
}

#[test]
fn the_calibration_material_is_complete_and_follows_the_normal_option() {
    let dir = std::env::temp_dir().join(format!("mc_mtlx_cal_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let r = mtlx::calibration(&dir, &Options { size: Some(64), ..Options::default() }).expect("calibration");
    let root = parse(&std::fs::read_to_string(&r.mtlx).unwrap()).unwrap();
    check_references(&root);
    assert!(r.files.len() >= 5 && r.files.iter().all(|f| f.is_file()));
    // a bump in the DirectX and the OpenGL file: the green channel of the top half is mirrored
    let dir2 = dir.join("gl");
    let r2 = mtlx::calibration(&dir2, &Options { size: Some(64), normal: NormalY::OpenGl, ..Options::default() }).unwrap();
    let a = std::fs::read(r.files.iter().find(|f| f.to_string_lossy().contains("normal")).unwrap()).unwrap();
    let b = std::fs::read(r2.files.iter().find(|f| f.to_string_lossy().contains("normal")).unwrap()).unwrap();
    assert_ne!(a, b, "the two conventions must differ");
    let _ = std::fs::remove_dir_all(&dir);
}
