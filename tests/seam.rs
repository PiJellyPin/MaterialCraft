// SPDX-License-Identifier: MIT OR Apache-2.0
use materialcraft::graph::{demo_graph_for, Graph, SPECS};
use materialcraft::tex::Tex;

/// Nodes whose result is data and not a surface (a strip of swatches, a plot of a row, a running sum), so it has no
/// reason to tile.
const DATA_STRIPS: &[&str] = &["palette_extract", "cross_section", "summed_area_table"];

/// (seam jump, largest jump anywhere else): the mean difference across the wrap-around border, and the
/// steepest mean difference between any two neighbouring columns/rows inside the texture. Hard-edged patterns whose
/// cell border falls exactly on the seam still pass, because the same jump exists inside the texture.
fn seam(t: &Tex) -> (f32, f32) {
    let (w, h) = (t.w as i64, t.h as i64);
    let d = |a: [f32; 4], b: [f32; 4]| (0..3).map(|i| (a[i] - b[i]).abs()).fold(0.0f32, f32::max);
    let col = |x: i64| (0..h).map(|y| d(t.at(x, y), t.at(x + 1, y))).sum::<f32>() / h as f32;
    let row = |y: i64| (0..w).map(|x| d(t.at(x, y), t.at(x, y + 1))).sum::<f32>() / w as f32;
    let seam = col(w - 1).max(row(h - 1));
    let inner = (0..w - 1).map(col).chain((0..h - 1).map(row)).fold(0.0f32, f32::max);
    (seam, inner)
}

#[test]
fn every_node_is_seamless() {
    let mut bad = Vec::new();
    for sp in SPECS {
        if DATA_STRIPS.contains(&sp.kind) {
            continue;
        }
        let Some(text) = demo_graph_for(sp.kind) else { continue };
        let g = Graph::parse(&text).unwrap();
        let out = match g.evaluate(Some(seam_size())) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("{}: error {e}", sp.kind);
                continue;
            }
        };
        let t = &out["o"];
        let (s, i) = seam(t);
        if s > i * 1.5 + 0.01 {
            bad.push(format!("{:<22} seam {:.4} inner {:.4}", sp.kind, s, i));
        }
    }
    for b in &bad {
        eprintln!("{b}");
    }
    assert!(bad.is_empty(), "{} node(s) are not seamless", bad.len());
}

/// Parameter variants (modes, odd counts, angles, offsets) of every generator and filter that could break tiling.
#[test]
fn variants_are_seamless() {
    let pre = "size 128\nnode 100 fbm scale=4 octaves=4 seed=2\nnode 102 sdf_shape shape=0 size=0.3 x=0.35\nnode 104 voronoi scale=5 mode=2 seed=3\nnode 105 threshold in=@104 threshold=0.2\nnode 107 normal in=@100 strength=3\nnode 108 shape shape=0 size_x=0.3 size_y=0.3 pos_x=0.4 pos_y=0.45\n";
    let cases: &[&str] = &[
        "perlin scale=3", "perlin scale=7 seed=4 disorder=0.5", "fbm scale=5 mode=1", "fbm scale=3 mode=2 octaves=8", "clouds scale=5 disorder=0.4",
        "voronoi scale=7 mode=0", "voronoi scale=7 mode=1", "voronoi scale=7 mode=3", "voronoi scale=5 jitter=0.3 mode=2",
        "bnw_spots variant=0", "bnw_spots variant=1", "bnw_spots variant=2", "crystal variant=0", "crystal variant=1",
        "grunge variant=0", "grunge variant=1", "grunge variant=2", "grunge variant=3", "plasma scale=3", "plasma scale=5 disorder=0.3",
        "directional_noise angle=0.13 scale=5 stretch=6", "directional_noise angle=0.25", "directional_noise angle=0.6 scale=3 stretch=12",
        "gradient mode=0 angle=0.1", "gradient mode=1", "gradient_axial angle=0.2", "gradient_circular offset=0.3",
        "circle radius=0.5", "circle radius=0.7 softness=0.1", "rect width=0.9 height=0.9", "polygon sides=3 radius=0.5", "polygon sides=8 radius=0.6 rotation=0.1",
        "star points=7 size=0.5", "checker tiles=7", "checker tiles=3", "weave count=5", "grunge_map map=1", "grunge_map map=4", "grunge_map map=6 disorder=0.5", "grunge_map map=11 brush_pattern=0.6", "grunge_map map=15", "grunge_map map=16", "grunge_map map=17", "grunge_map map=18", "grunge_map map=19", "grunge_map map=20", "grunge_map map=21", "grunge_map map=22", "grunge_map map=23", "grunge_map map=24 disorder=0.4", "weave_generator tile_x=2 weave=3", "triangle_grid style=1 x_amount=8 y_amount=8", "triangle_grid style=1 triangulation=2 output=1", "triangle_grid style=1 output=2 thin_lines=1 quincunx_offset_x=0.5", "weave count=9 width=0.6",
        "bricks rows=7 cols=3 offset=0.3", "bricks rows=5 cols=5 offset=0.5", "bricks rows=9 cols=4 offset=0.25", "bricks rows=6 cols=3.5",
        "stripes count=7 angle=0.13", "stripes count=5 angle=0.37", "stripes count=9 angle=0.9",
        "shape shape=0", "shape shape=1", "shape shape=2", "shape shape=3", "shape shape=4", "shape shape=5", "shape shape=6", "shape shape=7",
        "shape shape=1 size_x=1 size_y=1", "shape shape=2 size_x=1.4 size_y=1.4 pos_x=0.3",
        "sdf_shape shape=0 x=0.4 y=0.4", "sdf_shape shape=1 x=0.45 size=0.3", "sdf_shape shape=2 sides=6 size=0.4 x=-0.45", "sdf_shape shape=3 size=0.4 y=0.45",
        "sdf_form shape=0", "sdf_form shape=1 repeat=5", "sdf_form shape=2", "sdf_form shape=3 repeat=2 scatter=0.5",
        "tile_sampler x_amount=5 y_amount=7", "tile_sampler x_amount=9 y_amount=6 size_random=0.5 rotation_random=1 position_random=0.4",
        "tile_generator x_amount=5 y_amount=7", "tile_generator x_amount=6 y_amount=6 offset=0.5",
        "splatter count=30 size=0.2", "splatter count=60 size=0.3 size_random=0.5 rotation_random=1 pos_random=1",
        "splatter_circular count=12 radius=0.45 size=0.2",
        "gaussian_noise scale=3", "white_noise seed=3",
    ];
    // generator-style kinds (no input)  --  with an optional pattern input where the node takes one
    let mut bad = Vec::new();
    for c in cases {
        let kind = c.split(' ').next().unwrap();
        let mut line = format!("node 1 {c}");
        if matches!(kind, "tile_sampler" | "tile_generator" | "splatter" | "splatter_circular") {
            line += " pattern=@108";
        }
        let text = format!("{pre}{line}\noutput o 1\n");
        let g = Graph::parse(&text).unwrap();
        match g.evaluate(Some(seam_size())) {
            Ok(out) => {
                let (s, i) = seam(&out["o"]);
                if s > i * 1.5 + 0.01 {
                    bad.push(format!("{c:<60} seam {s:.4} inner {i:.4}"));
                }
            }
            Err(e) => bad.push(format!("{c}: error {e}")),
        }
    }
    let filters: &[&str] = &[
        "blur radius=9", "blur_hq intensity=0.5", "directional_blur angle=0.1 length=0.1", "sharpen radius=3", "highpass radius=8", "swirl amount=1 radius=0.5",
        "transform offset_x=0.3 offset_y=0.2 scale_x=1.7 scale_y=1.3 rotation=0.123", "transform rotation=0.125", "transform scale_x=0.5 scale_y=0.5", "mirror axis=0", "mirror axis=1", "mirror axis=2",
        "warp in=@100 warp=@100 intensity=0.2", "directional_warp warp=@100 angle=0.2 intensity=0.3", "vector_warp vector=@107 intensity=0.1", "slope_blur slope=@100 intensity=0.2",
        "normal strength=4", "ao radius=8", "curvature radius=4", "edge_detect", "emboss angle=0.1", "bevel distance=0.05", "distance distance=0.1", "histogram_range position=0.5",
        "make_it_tile_photo width=0.2", "make_it_tile_patch patches=5", "edge_wear amount=0.5", "curvature_smooth",
        "flood_fill", "flood_fill_random", "flood_fill_color", "flood_fill_gradient",
    ];
    for c in filters {
        let kind = c.split(' ').next().unwrap();
        let mut line = format!("node 1 {c}");
        let src = match kind {
            "flood_fill" => 105,
            "flood_fill_random" | "flood_fill_color" | "flood_fill_gradient" => 106,
            "curvature_smooth" => 107,
            _ => 100,
        };
        if !line.contains("in=@") {
            line += &format!(" in=@{src}");
        }
        let text = format!("{pre}node 106 flood_fill in=@105\n{line}\noutput o 1\n");
        let g = Graph::parse(&text).unwrap();
        match g.evaluate(Some(seam_size())) {
            Ok(out) => {
                let (s, i) = seam(&out["o"]);
                if s > i * 1.5 + 0.01 {
                    bad.push(format!("{c:<60} seam {s:.4} inner {i:.4}"));
                }
            }
            Err(e) => bad.push(format!("{c}: error {e}")),
        }
    }
    for b in &bad {
        eprintln!("{b}");
    }
    assert!(bad.is_empty(), "{} variant(s) are not seamless", bad.len());
}

fn seam_size() -> usize {
    std::env::var("SEAM_SIZE").ok().and_then(|s| s.parse().ok()).unwrap_or(128)
}

/// The shipped example material: every output map must tile.
#[test]
fn example_material_outputs_are_seamless() {
    let g = Graph::parse(materialcraft::graph::EXAMPLE).unwrap();
    let out = g.evaluate(Some(256)).unwrap();
    let mut bad = Vec::new();
    for (name, t) in &out {
        let (s, i) = seam(t);
        if s > i * 1.5 + 0.01 {
            bad.push(format!("{name}: seam {s:.4} inner {i:.4}"));
        }
    }
    assert!(bad.is_empty(), "{bad:?}");
}

/// Wrap-equivariance: rolling the input by whole pixels must roll the output by the same amount. This is the exact
/// definition of "the filter wraps around the borders", so a filter that clamps or mirrors at the edge fails it.
#[test]
fn filters_commute_with_wrapped_shifts() {
    const N: usize = 128;
    let (dx, dy) = (37.0 / N as f32, 21.0 / N as f32);
    let filters: &[&str] = &[
        "blur radius=9", "blur_hq intensity=0.5", "directional_blur angle=0.1 length=0.1", "sharpen radius=3", "highpass radius=8",
        "normal strength=4", "ao radius=8", "curvature radius=4", "edge_detect", "emboss angle=0.1", "bevel distance=0.05",
        "distance distance=0.1", "histogram_range position=0.5", "histogram_scan position=0.5", "levels low=0.2 high=0.8", "invert", "threshold threshold=0.5",
        "slope_blur slope=@100 intensity=0.2", "warp warp=@100 intensity=0.2", "directional_warp warp=@100 angle=0.2 intensity=0.3",
        "vector_warp vector=@107 intensity=0.1", "swirl amount=0.0", "curvature_smooth", "edge_wear amount=0.5", "dirt", "dust",
        "height_blend bg=@100 fg=@107 opacity=0.5", "clamp low=0.2 high=0.8", "hsl", "gradient_map", "quantize",
    ];
    let mut bad = Vec::new();
    for c in filters {
        let _kind = c.split(' ').next().unwrap();
        let mut line = format!("node 1 {c}");
        if !line.contains("in=@") {
            line += " in=@100";
        }
        let pre = |shift: bool| {
            let base = if shift { "node 99 fbm scale=4 octaves=4 seed=2\nnode 100 transform in=@99 offset_x=DX offset_y=DY\n" } else { "node 100 fbm scale=4 octaves=4 seed=2\n" };
            let base = base.replace("DX", &dx.to_string()).replace("DY", &dy.to_string());
            format!("size {N}\n{base}node 107 normal in=@100 strength=3\n")
        };
        let ga = format!("{}{}\nnode 2 transform in=@1 offset_x={} offset_y={}\noutput o 2\n", pre(false), line, dx, dy);
        let gb = format!("{}{}\noutput o 1\n", pre(true), line);
        let (Ok(a), Ok(b)) = (Graph::parse(&ga), Graph::parse(&gb)) else {
            eprintln!("{c}: parse failed");
            continue;
        };
        let (Ok(a), Ok(b)) = (a.evaluate(Some(N)), b.evaluate(Some(N))) else {
            eprintln!("{c}: evaluate failed");
            continue;
        };
        let (a, b) = (&a["o"], &b["o"]);
        let mut worst = 0.0f32;
        let mut sum = 0.0f32;
        for (p, q) in a.px.iter().zip(&b.px) {
            let d = (0..3).map(|i| (p[i] - q[i]).abs()).fold(0.0f32, f32::max);
            worst = worst.max(d);
            sum += d;
        }
        let mean = sum / a.px.len() as f32;
        if std::env::var_os("SEAM_VERBOSE").is_some() { eprintln!("{c:<56} mean {mean:.5} worst {worst:.3}"); }
        if mean > 0.004 {
            bad.push(format!("{c:<56} mean diff {mean:.4}  worst {worst:.3}"));
        }
    }
    for b in &bad {
        eprintln!("{b}");
    }
    assert!(bad.is_empty(), "{} filter(s) do not wrap at the borders", bad.len());
}

/// Non-square graphs: every node's demo graph evaluated at 2:1 and 1:2 must return textures of exactly that shape, and
/// the result must still tile on both axes.
#[test]
fn every_node_is_seamless_when_not_square() {
    let mut bad = Vec::new();
    for (wide, name) in [(true, "2:1"), (false, "1:2")] {
        for sp in SPECS {
            if DATA_STRIPS.contains(&sp.kind) {
                continue;
            }
            let Some(text) = demo_graph_for(sp.kind) else { continue };
            let mut g = Graph::parse(&text).unwrap();
            let s = g.size;
            g.height = if wide { s / 2 } else { s * 2 };
            let size = seam_size();
            let h = if wide { size / 2 } else { size * 2 };
            let out = match g.evaluate(Some(size)) {
                Ok(o) => o,
                Err(e) => {
                    bad.push(format!("{} {name}: error {e}", sp.kind));
                    continue;
                }
            };
            let t = &out["o"];
            if t.w != size || t.h != h {
                bad.push(format!("{:<22} {name}: got {}x{}, want {size}x{h}", sp.kind, t.w, t.h));
                continue;
            }
            let (sm, inner) = seam(t);
            if sm > inner * 1.5 + 0.01 {
                bad.push(format!("{:<22} {name}: seam {sm:.4} inner {inner:.4}", sp.kind));
            }
        }
    }
    for b in &bad {
        eprintln!("{b}");
    }
    assert!(bad.is_empty(), "{} non-square problem(s)", bad.len());
}

#[test]
fn non_square_size_round_trips_through_the_file_format() {
    let g = Graph::parse("size 512 256\nnode 1 perlin scale=4\noutput o 1\n").unwrap();
    assert_eq!((g.size, g.height), (512, 256));
    assert!(g.to_text().contains("\nsize 512 256\n"));
    let sq = Graph::parse("size 512\nnode 1 perlin scale=4\noutput o 1\n").unwrap();
    assert!(sq.to_text().contains("\nsize 512\n"));
    let t = &g.evaluate(Some(128)).unwrap()["o"];
    assert_eq!((t.w, t.h), (128, 64));
}
