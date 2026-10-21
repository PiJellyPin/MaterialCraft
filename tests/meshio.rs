// SPDX-License-Identifier: MIT OR Apache-2.0
use materialcraft::meshio::{self, NormalMode};
use materialcraft::v3::*;

fn area(m: &meshio::Mesh) -> f32 {
    m.tris.iter().map(|t| t.area()).sum()
}

#[test]
fn obj_polygons_indices_names_and_continuations() {
    // a concave L-shaped hexagon (area 3) with negative indices, an object, a group and a material
    let obj = "o thing\ng part\nusemtl steel\nv 0 0 0\nv 2 0 0\nv 2 1 0\nv 1 1 0\nv 1 2 0\nv 0 2 0\nf -6 -5 -4 -3 \\\n -2 -1\n";
    let m = meshio::parse_obj(obj).unwrap();
    assert_eq!(m.tris.len(), 4, "a hexagon makes four triangles");
    assert!((area(&m) - 3.0).abs() < 1e-5, "ear clipping keeps the L shape: {}", area(&m));
    assert_eq!(m.objects, vec!["thing"]);
    assert_eq!(m.groups, vec!["part"]);
    assert_eq!(m.materials, vec!["steel"]);
    assert!(m.tris.iter().all(|t| t.face_normal()[2] > 0.99), "winding is kept");
    // a quad is fanned, a convex pentagon too
    let q = meshio::parse_obj("v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nf 1 2 3 4\n").unwrap();
    assert_eq!(q.tris.len(), 2);
    // vertex colours after the position
    let c = meshio::parse_obj("v 0 0 0 1 0 0\nv 1 0 0 0 1 0\nv 0 1 0 0 0 1\nf 1 2 3\n").unwrap();
    assert!(c.has_colors && c.tris[0].col[1] == [0.0, 1.0, 0.0]);
    // errors
    assert!(meshio::parse_obj("v 0 0 0\n").is_err());
    assert!(meshio::parse_obj("v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 9\n").is_err());
    assert!(meshio::parse_obj("v 0 0 0\nv 1 0 0\nv 0 1 0\nf 0 1 2\n").is_err());
    assert!(meshio::parse_obj("v a b c\n").is_err());
}

#[test]
fn obj_without_normals_is_flat_or_smoothed_by_group() {
    let cube_face = |s: &str| format!("v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nv 1 0 1\nv 1 1 1\n{s}");
    // two quads at 90 degrees sharing an edge
    let flat = meshio::parse_obj(&cube_face("f 1 2 3 4\nf 2 5 6 3\n")).unwrap();
    assert!(!flat.has_normals);
    assert!(flat.tris.iter().all(|t| (len(sub(t.n[0], t.face_normal()))) < 1e-5), "no smoothing group: flat");
    let smooth = meshio::parse_obj(&cube_face("s 1\nf 1 2 3 4\nf 2 5 6 3\n")).unwrap();
    let t = &smooth.tris[0];
    assert!(len(sub(t.n[1], t.face_normal())) > 0.1, "the shared edge gets a blended normal");
    assert!(len(sub(t.n[0], t.face_normal())) < 1e-5, "a corner that is not shared keeps its face normal");
}

#[test]
fn normal_modes() {
    let sphere = meshio::parse_obj(&materialcraft::bake::uv_sphere_obj(16, 10, 1.0, 0.0)).unwrap();
    let mut flat = sphere.clone();
    flat.apply_normals(NormalMode::Flat);
    assert!(flat.tris.iter().all(|t| len(sub(t.n[0], t.face_normal())) < 1e-5));
    let mut smooth = sphere.clone();
    smooth.apply_normals(NormalMode::Angle(80.0));
    // smoothed normals of a sphere point along the position
    let bad = smooth.tris.iter().filter(|t| (0..3).any(|c| dot(t.n[c], norm(t.p[c])) < 0.97 && len(t.p[c]) > 0.5)).count();
    assert!(bad * 20 < smooth.tris.len(), "{bad} of {} triangles have off normals", smooth.tris.len());
    // a cube keeps hard edges below the angle limit and smooths them above it
    let cube = meshio::parse_obj("v -1 -1 -1\nv 1 -1 -1\nv 1 1 -1\nv -1 1 -1\nv -1 -1 1\nv 1 -1 1\nv 1 1 1\nv -1 1 1\nf 1 4 3 2\nf 5 6 7 8\nf 1 2 6 5\nf 2 3 7 6\nf 3 4 8 7\nf 4 1 5 8\n").unwrap();
    let mut hard = cube.clone();
    hard.apply_normals(NormalMode::Angle(30.0));
    assert!(hard.tris.iter().all(|t| len(sub(t.n[1], t.face_normal())) < 1e-4));
    let mut soft = cube.clone();
    soft.apply_normals(NormalMode::Angle(120.0));
    assert!(soft.tris.iter().any(|t| len(sub(t.n[1], t.face_normal())) > 0.3));
}

fn ply_ascii() -> String {
    "ply\nformat ascii 1.0\ncomment test\nelement vertex 4\nproperty float x\nproperty float y\nproperty float z\nproperty float nx\nproperty float ny\nproperty float nz\nproperty float s\nproperty float t\nproperty uchar red\nproperty uchar green\nproperty uchar blue\nelement face 1\nproperty list uchar int vertex_indices\nend_header\n0 0 0 0 0 1 0 0 255 0 0\n1 0 0 0 0 1 1 0 0 255 0\n1 1 0 0 0 1 1 1 0 0 255\n0 1 0 0 0 1 0 1 255 255 255\n4 0 1 2 3\n".to_string()
}

#[test]
fn ply_ascii_and_binary() {
    let m = meshio::parse_ply(ply_ascii().as_bytes()).unwrap();
    assert_eq!(m.tris.len(), 2);
    assert!(m.has_normals && m.has_uv && m.has_colors);
    assert!((area(&m) - 1.0).abs() < 1e-6);
    assert_eq!(m.tris[0].uv[1], [1.0, 0.0]);
    assert!((m.tris[0].col[1][1] - 1.0).abs() < 1e-6);
    for big in [false, true] {
        let mut b = format!("ply\nformat {} 1.0\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\nelement face 1\nproperty list uchar uint vertex_indices\nend_header\n", if big { "binary_big_endian" } else { "binary_little_endian" }).into_bytes();
        for v in [[0.0f32, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]] {
            for c in v {
                b.extend(if big { c.to_be_bytes() } else { c.to_le_bytes() });
            }
        }
        b.push(3);
        for i in [0u32, 1, 2] {
            b.extend(if big { i.to_be_bytes() } else { i.to_le_bytes() });
        }
        let m = meshio::parse_ply(&b).unwrap();
        assert_eq!(m.tris.len(), 1);
        assert!((area(&m) - 2.0).abs() < 1e-6, "big endian {big}");
    }
    assert!(meshio::parse_ply(b"hello").is_err());
    assert!(meshio::parse_ply(b"ply\nformat ascii 1.0\nelement vertex 1\nproperty float x\nend_header\n").is_err(), "truncated ASCII PLY must be rejected");
    // a face that points at a missing vertex is an error, not a panic
    let bad = ply_ascii().replace("4 0 1 2 3", "4 0 1 2 9");
    assert!(meshio::parse_ply(bad.as_bytes()).is_err());
    // truncated binary data
    let mut t = format!("ply\nformat binary_little_endian 1.0\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\nend_header\n").into_bytes();
    t.extend([0u8; 10]);
    assert!(meshio::parse_ply(&t).is_err());
}

#[test]
fn stl_ascii_and_binary() {
    let a = "solid t\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid\n";
    let m = meshio::parse_stl(a.as_bytes()).unwrap();
    assert_eq!(m.tris.len(), 1);
    assert!((area(&m) - 0.5).abs() < 1e-6);
    let mut b = vec![0u8; 80];
    b.extend(2u32.to_le_bytes());
    for k in 0..2 {
        for c in [0.0f32, 0.0, 1.0] {
            b.extend(c.to_le_bytes());
        }
        for v in [[0.0f32, 0.0, k as f32], [1.0, 0.0, k as f32], [0.0, 1.0, k as f32]] {
            for c in v {
                b.extend(c.to_le_bytes());
            }
        }
        b.extend([0u8, 0]);
    }
    let m = meshio::parse_stl(&b).unwrap();
    assert_eq!(m.tris.len(), 2);
    assert!(meshio::parse_stl(b"solid x\nvertex 0 0 0\n").is_err());
    let loaded = meshio::load_bytes("part.stl", &b, None).unwrap();
    assert_eq!(loaded.objects, vec!["part"], "a file without object names is named after the file");
}

fn b64(d: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for c in d.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    s
}

/// One triangle (positions, normals, uvs, u16 indices) as glTF JSON with the buffer inlined or left out for a GLB.
fn gltf_triangle(node_extra: &str, inline: bool) -> (String, Vec<u8>) {
    let mut bin: Vec<u8> = Vec::new();
    for v in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
        for c in v {
            bin.extend(c.to_le_bytes());
        }
    }
    for _ in 0..3 {
        for c in [0.0f32, 0.0, 1.0] {
            bin.extend(c.to_le_bytes());
        }
    }
    for v in [[0.0f32, 0.0], [1.0, 0.0], [0.0, 1.0]] {
        for c in v {
            bin.extend(c.to_le_bytes());
        }
    }
    for i in [0u16, 1, 2] {
        bin.extend(i.to_le_bytes());
    }
    bin.extend([0u8, 0]); // pad to 4
    let uri = if inline { format!(",\"uri\":\"data:application/octet-stream;base64,{}\"", b64(&bin)) } else { String::new() };
    let json = format!(
        "{{\"asset\":{{\"version\":\"2.0\"}},\"scene\":0,\"scenes\":[{{\"nodes\":[0]}}],\"nodes\":[{{\"name\":\"tri\",\"mesh\":0{node_extra}}}],\"meshes\":[{{\"primitives\":[{{\"attributes\":{{\"POSITION\":0,\"NORMAL\":1,\"TEXCOORD_0\":2}},\"indices\":3,\"material\":0}}]}}],\"materials\":[{{\"name\":\"paint\"}}],\"buffers\":[{{\"byteLength\":{}{uri}}}],\"bufferViews\":[{{\"buffer\":0,\"byteOffset\":0,\"byteLength\":36}},{{\"buffer\":0,\"byteOffset\":36,\"byteLength\":36}},{{\"buffer\":0,\"byteOffset\":72,\"byteLength\":24}},{{\"buffer\":0,\"byteOffset\":96,\"byteLength\":6}}],\"accessors\":[{{\"bufferView\":0,\"componentType\":5126,\"count\":3,\"type\":\"VEC3\"}},{{\"bufferView\":1,\"componentType\":5126,\"count\":3,\"type\":\"VEC3\"}},{{\"bufferView\":2,\"componentType\":5126,\"count\":3,\"type\":\"VEC2\"}},{{\"bufferView\":3,\"componentType\":5123,\"count\":3,\"type\":\"SCALAR\"}}]}}",
        bin.len()
    );
    (json, bin)
}

#[test]
fn gltf_embedded_glb_transforms_and_errors() {
    let (json, bin) = gltf_triangle(",\"translation\":[10,0,0],\"scale\":[2,2,2]", true);
    let m = meshio::parse_gltf(json.as_bytes(), None).unwrap();
    assert_eq!(m.tris.len(), 1);
    assert_eq!(m.objects, vec!["tri"]);
    assert_eq!(m.materials, vec!["paint"]);
    assert_eq!(m.tris[0].p[1], [12.0, 0.0, 0.0]);
    assert!((area(&m) - 2.0).abs() < 1e-5, "scale 2 makes the area four times 0.5");
    assert_eq!(m.tris[0].uv[2], [0.0, 0.0], "glTF v is flipped to the OBJ convention");
    assert!(m.has_normals && m.has_uv);
    // a mirrored node flips the winding so the surface still faces the same way as its normals
    let (mj, _) = gltf_triangle(",\"scale\":[-1,1,1]", true);
    let mm = meshio::parse_gltf(mj.as_bytes(), None).unwrap();
    assert!(dot(mm.tris[0].face_normal(), mm.tris[0].n[0]) > 0.9, "winding follows the normals");
    // GLB container with a BIN chunk
    let (gj, _) = gltf_triangle("", false);
    let mut jbytes = gj.into_bytes();
    while jbytes.len() % 4 != 0 {
        jbytes.push(b' ');
    }
    let mut glb = b"glTF".to_vec();
    glb.extend(2u32.to_le_bytes());
    glb.extend(((12 + 8 + jbytes.len() + 8 + bin.len()) as u32).to_le_bytes());
    glb.extend((jbytes.len() as u32).to_le_bytes());
    glb.extend(b"JSON");
    glb.extend(&jbytes);
    glb.extend((bin.len() as u32).to_le_bytes());
    glb.extend(b"BIN\0");
    glb.extend(&bin);
    let g = meshio::load_bytes("x.glb", &glb, None).unwrap();
    assert_eq!(g.tris.len(), 1);
    // broken files
    assert!(meshio::parse_gltf(b"{\"asset\":{}", None).is_err());
    assert!(meshio::parse_gltf(b"{}", None).is_err());
    let cut = json.replace("\"byteLength\":6}", "\"byteLength\":600}");
    assert!(meshio::parse_gltf(cut.as_bytes(), None).is_err(), "a view past the end of its buffer");
    let ext = json.replace("data:application/octet-stream;base64,", "../outside.bin#");
    assert!(meshio::parse_gltf(ext.as_bytes(), None).is_err());
    assert!(meshio::load_bytes("a.xyz", b"", None).is_err());
}

#[test]
fn gltf_external_buffer_and_path_safety() {
    let dir = std::env::temp_dir().join(format!("mc_gltf_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (json, bin) = gltf_triangle("", false);
    std::fs::write(dir.join("tri.bin"), &bin).unwrap();
    let ok = json.replace("\"buffers\":[{\"byteLength\":", "\"buffers\":[{\"uri\":\"tri.bin\",\"byteLength\":");
    std::fs::write(dir.join("tri.gltf"), &ok).unwrap();
    let m = meshio::load_file(&dir.join("tri.gltf")).unwrap();
    assert_eq!(m.tris.len(), 1);
    let evil = json.replace("\"buffers\":[{\"byteLength\":", "\"buffers\":[{\"uri\":\"../tri.bin\",\"byteLength\":");
    std::fs::write(dir.join("evil.gltf"), evil).unwrap();
    assert!(meshio::load_file(&dir.join("evil.gltf")).is_err());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn json_parser() {
    let j = meshio::parse_json("{\"a\":[1,2.5e1,-3],\"b\":\"x\\ny\\u00e9\\ud83d\\ude00\",\"c\":true,\"d\":null}").unwrap();
    assert_eq!(j.get("a").unwrap().arr().len(), 3);
    assert_eq!(j.get("a").unwrap().arr()[1].num(), Some(25.0));
    assert_eq!(j.get("b").unwrap().str(), Some("x\nyé😀"));
    for bad in ["", "{", "[1,]", "{\"a\" 1}", "[1] 2", "\"abc", "tru", "{\"a\":}", "[\"\\q\"]"] {
        assert!(meshio::parse_json(bad).is_err(), "should reject {bad:?}");
    }
    let deep = "[".repeat(200) + &"]".repeat(200);
    assert!(meshio::parse_json(&deep).is_err(), "deep nesting is refused instead of overflowing the stack");
}

#[test]
fn merge_keeps_names_and_counts() {
    let a = meshio::parse_obj("o rock_low\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
    let b = meshio::parse_obj("o rock_high\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
    let m = meshio::Mesh::merge(&[a, b]);
    assert_eq!(m.tris.len(), 2);
    assert_eq!(m.objects, vec!["rock_low", "rock_high"]);
    assert_eq!((m.tris[0].object, m.tris[1].object), (0, 1));
    let st = m.stats();
    assert_eq!((st.tris, st.objects), (2, 2));
}
