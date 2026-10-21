// SPDX-License-Identifier: MIT OR Apache-2.0
//! Resources: fonts (TrueType, OpenType/CFF, WOFF, collections), lookup tables, text wrapping, and finding linked
//! files.
use materialcraft::font::Font;
use materialcraft::graph::Graph;
use materialcraft::lut::Lut;
use materialcraft::resources;
use materialcraft::tex::Tex;
use materialcraft::vector::{fill, Xf};
use std::path::PathBuf;
use std::sync::Mutex;

static SERIAL: Mutex<()> = Mutex::new(());

fn data(n: &str) -> Vec<u8> {
    std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/assets").join(n)).unwrap()
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("mc_res_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

// ------------------------------------------------------------------------------------------------ fonts
fn coverage(f: &Font, text: &str) -> f32 {
    let polys = f.layout(text, 100.0, 1, 0.0, 1.0, &Xf::translate(100.0, 100.0));
    fill(&polys, 200, 200, false).a.iter().sum()
}

#[test]
fn font_formats_agree() {
    // 'A' is a 500x700 rectangle and 'B' a circle of radius 300 at 100 px per em
    for file in ["tiny.ttf", "tiny.otf", "tiny.woff", "tiny.ttc"] {
        let f = Font::load(&data(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
        let a = coverage(&f, "A");
        assert!((a - 3500.0).abs() < 60.0, "{file}: A covers {a}");
        let b = coverage(&f, "B");
        let want = std::f32::consts::PI * 900.0;
        assert!((b - want).abs() < want * 0.02, "{file}: B covers {b}");
        let description = f.describe();
        assert!(description.contains("MCTest"), "{file}: {description}");
        let format = if file == "tiny.otf" { "CFF" } else { "TrueType" };
        assert!(description.contains(format), "{file}: missing {format} in {description}");
    }
}

#[test]
fn bad_fonts_are_errors_not_panics() {
    assert!(Font::load(b"").is_err());
    assert!(Font::load(b"wOF2....").is_err());
    let mut b = data("tiny.otf");
    for i in (0..b.len()).step_by(7) {
        let keep = b[i];
        b[i] ^= 0xa5;
        let _ = Font::load(&b).map(|f| coverage(&f, "AB"));
        b[i] = keep;
    }
    let mut w = data("tiny.woff");
    for i in (0..w.len()).step_by(5) {
        let keep = w[i];
        w[i] ^= 0x5a;
        let _ = Font::load(&w).map(|f| coverage(&f, "AB"));
        w[i] = keep;
    }
}

#[test]
fn text_wraps_to_width() {
    let f = Font::Builtin;
    let t = f.wrap("aaa bbb ccc ddd", 4.0 * 0.65 + 0.001, 0.0);
    for line in t.lines() {
        assert!(f.text_width(line, 0.0) <= 4.0 * 0.65 + 0.01, "line '{line}'");
    }
    assert!(t.lines().count() >= 4);
    assert_eq!(f.wrap("one two", 0.0, 0.0), "one two");
    // long words are broken, explicit breaks are kept
    let t = f.wrap("abcdefghij\nxy", 3.0 * 0.65 + 0.001, 0.0);
    assert!(t.lines().count() >= 4 && t.lines().last() == Some("xy"));
}

#[test]
fn text_node_wrap_and_colour() {
    let plain = "size 64\nnode 1 text text=\"HELLO WORLD HELLO\" size=0.15\noutput o 1\n";
    let wrapped = "size 64\nnode 1 text text=\"HELLO WORLD HELLO\" size=0.15 wrap=0.5\noutput o 1\n";
    let a = Graph::parse(plain).unwrap().evaluate(None).unwrap()["o"].clone();
    let b = Graph::parse(wrapped).unwrap().evaluate(None).unwrap()["o"].clone();
    assert_ne!(a.px, b.px);
    // wrapped text is taller than one line: some ink in rows where the plain version has none
    let rows = |t: &Tex| (0..t.h).filter(|y| (0..t.w).any(|x| t.px[y * t.w + x][0] > 0.5)).count();
    assert!(rows(&b) > rows(&a));
    let c = Graph::parse("size 32\nnode 1 text text=\"MM\" size=0.8 use_color=1 color_r=1 color_g=0.5 color_b=0\noutput o 1\n").unwrap().evaluate(None).unwrap()["o"].clone();
    let p = c.px.iter().cloned().max_by(|x, y| x[0].partial_cmp(&y[0]).unwrap()).unwrap();
    assert!((p[0] - 1.0).abs() < 1e-3 && (p[1] - 0.5).abs() < 1e-3 && p[2] < 1e-3);
}

// ------------------------------------------------------------------------------------------------ lookup tables
fn probe() -> Vec<[f32; 3]> {
    let mut v = Vec::new();
    for i in 0..7 {
        for j in 0..7 {
            for k in 0..7 {
                v.push([i as f32 / 6.0 * 0.97, j as f32 / 6.0, k as f32 / 6.0 * 0.9 + 0.03]);
            }
        }
    }
    v
}

#[test]
fn identity_lut_is_identity() {
    let l = Lut::identity(5);
    for c in probe() {
        let o = l.apply(c);
        for k in 0..3 {
            assert!((o[k] - c[k]).abs() < 1e-5, "{c:?} -> {o:?}");
        }
    }
}

#[test]
fn cube_round_trip_and_grade() {
    // a table that swaps red and blue and halves green
    let n = 4;
    let mut text = format!("TITLE \"swap\"\nLUT_3D_SIZE {n}\n");
    for b in 0..n {
        for g in 0..n {
            for r in 0..n {
                let m = (n - 1) as f32;
                text.push_str(&format!("{} {} {}\n", b as f32 / m, g as f32 / m * 0.5, r as f32 / m));
            }
        }
    }
    let l = Lut::parse_cube(&text).unwrap();
    assert!(l.is_3d() && l.size() == 4);
    for c in probe() {
        let o = l.apply(c);
        let want = [c[2], c[1] * 0.5, c[0]];
        for k in 0..3 {
            assert!((o[k] - want[k]).abs() < 1e-4, "{c:?} -> {o:?} want {want:?}");
        }
    }
    let l2 = Lut::parse_cube(&l.to_cube("again")).unwrap();
    assert!((l2.apply([0.3, 0.6, 0.9])[0] - 0.9).abs() < 1e-3);
}

#[test]
fn cube_1d_domain_and_errors() {
    let l = Lut::parse_cube("LUT_1D_SIZE 3\nDOMAIN_MIN 0 0 0\nDOMAIN_MAX 2 2 2\n0 0 0\n0.5 0.25 1\n1 1 0\n").unwrap();
    let o = l.apply([1.0, 1.0, 1.0]);
    assert!((o[0] - 0.5).abs() < 1e-5 && (o[1] - 0.25).abs() < 1e-5 && (o[2] - 1.0).abs() < 1e-5);
    assert!(Lut::parse_cube("LUT_3D_SIZE 2\n0 0 0\n").is_err());
    assert!(Lut::parse_cube("0 0 0\n").is_err());
    assert!(Lut::parse_cube("LUT_3D_SIZE 2\nx y z\n").is_err());
}

#[test]
fn lut_3dl_blue_fastest() {
    let mut text = String::from("0 341 682 1023\n");
    for r in 0..4 {
        for g in 0..4 {
            for b in 0..4 {
                // output = (blue, green, red) scaled to 10 bits
                text.push_str(&format!("{} {} {}\n", b * 341, g * 341, r * 341));
            }
        }
    }
    let l = Lut::parse_3dl(&text).unwrap();
    let o = l.apply([1.0, 0.0, 0.0]);
    // red input 1 -> output channel 2 is full
    assert!(o[2] > 0.99 && o[0] < 0.01, "{o:?}");
}

#[test]
fn lut_from_strip_and_hald() {
    let n = 4;
    let id = Lut::identity(n);
    let Lut::D3 { data: d, .. } = &id else { panic!() };
    // strip: x = b*n + r, y = g
    let mut strip = Tex::solid(n * n, n, [0.0; 4]);
    for b in 0..n {
        for g in 0..n {
            for r in 0..n {
                let c = d[r + g * n + b * n * n];
                strip.px[g * n * n + b * n + r] = [c[0], c[1], c[2], 1.0];
            }
        }
    }
    let l = Lut::from_image(&strip).unwrap();
    assert!((l.apply([0.2, 0.7, 0.4])[1] - 0.7).abs() < 1e-5);
    // Hald level 2: 8x8 image holding a 4x4x4 table in scanline order
    let mut hald = Tex::solid(8, 8, [0.0; 4]);
    for (i, c) in d.iter().enumerate() {
        hald.px[i] = [c[0], c[1], c[2], 1.0];
    }
    let l = Lut::from_image(&hald).unwrap();
    assert!(l.size() == 4 && (l.apply([0.9, 0.1, 0.5])[0] - 0.9).abs() < 1e-5);
    assert!(Lut::from_image(&Tex::solid(5, 7, [0.0; 4])).is_err());
}

#[test]
fn color_lut_node() {
    // swap red and blue through a .cube file embedded in the graph
    let n = 2;
    let mut cube = format!("LUT_3D_SIZE {n}\n");
    for b in 0..n {
        for g in 0..n {
            for r in 0..n {
                cube.push_str(&format!("{b} {g} {r}\n"));
            }
        }
    }
    let mut g = Graph::default();
    g.size = 16;
    g.resources.insert("swap.cube".into(), std::sync::Arc::new(cube.into_bytes()));
    let text = g.to_text() + "node 1 color r=1 g=0.25 b=0\nnode 2 color_lut in=@1 file=\"swap.cube\"\noutput o 2\n";
    let g = Graph::parse(&text).unwrap();
    let t = &g.evaluate(None).unwrap()["o"];
    let p = t.px[0];
    assert!(p[0] < 0.01 && (p[1] - 0.25).abs() < 0.01 && (p[2] - 1.0).abs() < 0.01, "{p:?}");
    // a missing file is a clear error
    assert!(Graph::parse("size 8\nnode 1 color\nnode 2 color_lut in=@1\noutput o 2\n").is_err());
}

// ------------------------------------------------------------------------------------------------ bitmap formats in nodes
#[test]
fn bitmap_node_reads_every_format() {
    for file in ["base.jpg", "lossy90.webp", "lossless.webp", "tiff_lzw.tif", "t.bmp", "t.tga", "d5.dds"] {
        let mut g = Graph::default();
        g.size = 32;
        g.resources.insert(file.into(), std::sync::Arc::new(data(file)));
        let text = g.to_text() + &format!("node 1 bitmap file=\"{file}\"\noutput o 1\n");
        let out = Graph::parse(&text).unwrap().evaluate(None).unwrap();
        assert_eq!(out["o"].w, 32, "{file}");
        assert!(out["o"].px.iter().any(|p| p[0] > 0.1), "{file}");
    }
}

// ------------------------------------------------------------------------------------------------ linked files
#[test]
fn linked_files_are_found_in_the_search_folders() {
    let _g = SERIAL.lock().unwrap();
    let proj = tmp("search_dirs");
    std::fs::create_dir_all(proj.join("resources")).unwrap();
    std::fs::write(proj.join("resources/pic one.webp"), data("lossless.webp")).unwrap();
    std::fs::write(proj.join("art.svg"), "<svg viewBox='0 0 10 10'><circle cx='5' cy='5' r='4'/></svg>").unwrap();
    resources::set_search_dirs(vec![proj.clone(), proj.join("resources")]);
    // relative to a search folder, by file name when the stored path no longer exists, and with backslashes
    assert!(resources::resolve("art.svg").is_some());
    assert!(resources::resolve("/old/place/pic one.webp").is_some());
    assert!(resources::resolve("resources\\pic one.webp").is_some());
    assert!(resources::resolve("nope.png").is_none());
    assert!(resources::read_file("nope.png").unwrap_err().contains("not found"));
    let g = Graph::parse_unchecked("size 32\nnode 1 bitmap file=\"pic one.webp\"\nnode 2 svg file=\"art.svg\"\nnode 3 blend fg=@2 bg=@1\noutput o 3\n").unwrap();
    g.evaluate(None).unwrap();
    resources::set_search_dirs(vec![]);
    assert!(resources::resolve("art.svg").is_none() || std::path::Path::new("art.svg").is_file());
}
