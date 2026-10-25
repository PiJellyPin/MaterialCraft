// SPDX-License-Identifier: MIT OR Apache-2.0
//! The speed-ups must not change results: the worker pool, the shared runs of multi-output nodes, the cache passes and
//! the rewritten kernels are checked against straightforward versions here.
use materialcraft::graph::{Graph, NodeCache};
use materialcraft::tex::{Plane, Tex};
use materialcraft::{noise, ops, pool};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[test]
fn library_nodes_do_not_mutate_shared_input_pixels() {
    use materialcraft::{graph::Node, nodes10};
    let input = Arc::new(Tex::from_fn(32, 24, |u, v| [u, v, u * v, 0.75]));
    let original = input.px.clone();
    let inputs = std::collections::BTreeMap::from([("input".into(), input.clone())]);
    for kind in ["x_exposure", "x_median_filter_color", "x_mlv_color"] {
        let mut node = Node::default();
        node.nums.insert("exposure".into(), 1.0);
        let mut output = nodes10::compute(kind, 1, &node, &inputs, 32, 24, 0).unwrap().unwrap();
        assert_eq!((output.w, output.h), (32, 24));
        assert_eq!(input.px, original, "{kind} mutated the shared input");
        output.px[0] = [9.0; 4];
        assert_eq!(input.px, original, "{kind} output aliases its input");
    }
    let black = nodes10::compute("x_exposure", 1, &Default::default(), &Default::default(), 32, 24, 0).unwrap().unwrap();
    assert_eq!((black.w, black.h), (32, 24));
    assert!(black.px.iter().all(|p| *p == [0.0, 0.0, 0.0, 1.0]));
}

#[test]
fn pool_runs_every_piece_once_also_nested_and_from_several_threads() {
    for n in [0usize, 1, 2, 7, 64, 1000] {
        let hits: Vec<AtomicUsize> = (0..n).map(|_| AtomicUsize::new(0)).collect();
        pool::for_each(n, &|i| {
            hits[i].fetch_add(1, Ordering::Relaxed);
        });
        assert!(hits.iter().all(|h| h.load(Ordering::Relaxed) == 1), "n = {n}");
    }
    // a job inside a job (the workers are busy) still runs every inner piece once
    let total = AtomicUsize::new(0);
    pool::for_each(8, &|_| {
        pool::for_each(50, &|_| {
            total.fetch_add(1, Ordering::Relaxed);
        });
    });
    assert_eq!(total.load(Ordering::Relaxed), 400);
    // several threads starting jobs at the same time (one gets the workers, the others use their own threads)
    let sum = AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..4 {
            s.spawn(|| {
                for _ in 0..20 {
                    pool::for_each(100, &|i| {
                        sum.fetch_add(i, Ordering::Relaxed);
                    });
                }
            });
        }
    });
    assert_eq!(sum.load(Ordering::Relaxed), 4 * 20 * (99 * 100 / 2));
}

#[test]
fn pool_passes_a_panic_on_and_keeps_working() {
    let r = std::panic::catch_unwind(|| {
        pool::for_each(64, &|i| {
            if i == 37 {
                panic!("piece 37");
            }
        });
    });
    assert!(r.is_err(), "the panic must reach the caller");
    let n = AtomicUsize::new(0);
    pool::for_each(64, &|_| {
        n.fetch_add(1, Ordering::Relaxed);
    });
    assert_eq!(n.load(Ordering::Relaxed), 64, "the pool must still work after a panic");
}

#[test]
fn for_chunks_covers_the_buffer_with_the_right_indices() {
    let mut v = vec![0usize; 1003];
    pool::for_chunks(&mut v, 10, &|ci, piece| {
        for (j, x) in piece.iter_mut().enumerate() {
            *x = ci * 10 + j;
        }
    });
    assert!(v.iter().enumerate().all(|(i, x)| i == *x));
}

#[test]
fn from_fn_gives_every_pixel_its_own_coordinates() {
    for (w, h) in [(1usize, 1usize), (3, 200), (257, 5), (64, 64)] {
        let t = Tex::from_fn(w, h, |u, v| [u, v, 0.0, 1.0]);
        for y in 0..h {
            for x in 0..w {
                let p = t.px[y * w + x];
                assert_eq!(p[0], (x as f32 + 0.5) / w as f32);
                assert_eq!(p[1], (y as f32 + 0.5) / h as f32);
            }
        }
    }
}

#[test]
fn plane_sampling_equals_texture_sampling() {
    let t = Tex::from_fn(37, 23, |u, v| {
        let a = noise::fbm(u, v, 3, 3, 4, 0.5, 9, 0);
        [a, a * 0.5, 1.0 - a, 1.0]
    });
    for c in 0..4 {
        let p = Plane::of(&t, c);
        for i in 0..2000 {
            let (u, v) = ((i as f32 * 0.6180339) * 3.1 - 1.3, (i as f32 * 0.4142136) * 2.7 - 0.9);
            assert_eq!(p.sample(u, v).to_bits(), t.sample(u, v)[c].to_bits(), "channel {c} at {u},{v}");
        }
    }
}

/// The exact small-radius gaussian, written the plain way (single thread, wrapped `at`).
fn blur_reference(t: &Tex, sigma: f32) -> Tex {
    let (w, h) = (t.w, t.h);
    let r = ((sigma * 3.0).ceil() as i64).clamp(1, 64);
    let mut k: Vec<f32> = (-r..=r).map(|i| (-((i * i) as f32) / (2.0 * sigma * sigma)).exp()).collect();
    let s: f32 = k.iter().sum();
    k.iter_mut().for_each(|x| *x /= s);
    let mut tmp = t.clone();
    for y in 0..h {
        for x in 0..w {
            let mut a = [0.0f32; 4];
            for (i, kv) in k.iter().enumerate() {
                let p = t.at(x as i64 + i as i64 - r, y as i64);
                for c in 0..4 {
                    a[c] += p[c] * kv;
                }
            }
            tmp.px[y * w + x] = a;
        }
    }
    let mut out = tmp.clone();
    for y in 0..h {
        for x in 0..w {
            let mut a = [0.0f32; 4];
            for (i, kv) in k.iter().enumerate() {
                let p = tmp.at(x as i64, y as i64 + i as i64 - r);
                for c in 0..4 {
                    a[c] += p[c] * kv;
                }
            }
            out.px[y * w + x] = a;
        }
    }
    out
}

#[test]
fn small_blur_equals_the_plain_kernel() {
    let t = Tex::from_fn(41, 29, |u, v| [noise::fbm(u, v, 4, 4, 3, 0.5, 2, 0), u, v, 0.5 + 0.5 * (u * 9.0).sin()]);
    for sigma in [0.3f32, 1.0, 1.7, 2.4] {
        let (a, b) = (ops::blur(&t, sigma), blur_reference(&t, sigma));
        assert_eq!(a.px, b.px, "sigma {sigma}");
    }
}

/// Horizon-based occlusion written the plain way (four-channel sampling, step values recomputed in the loop).
fn hbao_reference(h: &Tex, radius_uv: f32, height_scale: f32, dirs: u32, intensity: f32) -> Tex {
    let g = ops::to_gray(h);
    let (w, hh) = (g.w, g.h);
    let dirs = dirs.clamp(2, 64) as usize;
    let steps = 14usize;
    let tab: Vec<(f32, f32)> = (0..dirs).map(|k| ((k as f32 / dirs as f32 * std::f32::consts::TAU).cos(), (k as f32 / dirs as f32 * std::f32::consts::TAU).sin())).collect();
    let mut o = g.clone();
    for y in 0..hh {
        for x in 0..w {
            let (u, v) = ((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / hh as f32);
            let h0 = g.px[y * w + x][0];
            let mut occ = 0.0f32;
            for &(dx, dy) in &tab {
                let mut best = 0.0f32;
                for j in 1..=steps {
                    let t = j as f32 / steps as f32;
                    let d = radius_uv * t;
                    let hs = g.sample(u + dx * d, v + dy * d)[0];
                    let dz = (hs - h0) * height_scale;
                    let s = dz / (dz * dz + d * d).sqrt().max(1e-9);
                    best = best.max(s * (1.0 - t * t));
                }
                occ += best;
            }
            let a = (1.0 - occ / dirs as f32 * intensity).clamp(0.0, 1.0);
            o.px[y * w + x] = [a, a, a, 1.0];
        }
    }
    o
}

#[test]
fn horizon_occlusion_equals_the_plain_version() {
    // The fast version shares the bilinear weights of all pixels (every pixel looks at the same offsets), so it rounds
    // differently from the plain one in the last bits; the results must agree to well below one 8-bit level. The
    // sizes include odd and non-square ones so the wrap-around runs at the borders are covered.
    for (w, h) in [(48usize, 40usize), (37, 53), (16, 16), (5, 7)] {
        let t = Tex::gray_fn(w, h, |u, v| noise::fbm(u, v, 3, 3, 5, 0.55, 4, 0));
        for (r, s, d) in [(0.05f32, 0.1f32, 8u32), (0.2, 0.4, 4), (0.12, 0.05, 16), (0.25, 0.3, 8)] {
            let (a, b) = (ops::hbao(&t, r, s, d, 1.0), hbao_reference(&t, r, s, d, 1.0));
            assert_eq!((a.w, a.h), (b.w, b.h));
            let worst = a.px.iter().zip(&b.px).map(|(x, y)| (x[0] - y[0]).abs().max((x[3] - y[3]).abs())).fold(0.0f32, f32::max);
            assert!(worst < 2e-4, "{w}x{h} radius {r} scale {s} dirs {d}: largest difference {worst}");
        }
        // no search distance, nothing to occlude (the plain version only has rounding noise to show here)
        let none = ops::hbao(&t, 0.0, 0.2, 4, 1.0);
        assert!(none.px.iter().all(|p| p[0] == 1.0), "{w}x{h}: a zero radius occludes nothing");
    }
}

#[test]
fn ambient_occlusion_and_curvature_equal_the_plain_formula() {
    let t = Tex::gray_fn(40, 36, |u, v| noise::fbm(u, v, 4, 4, 4, 0.5, 7, 0));
    let g = ops::to_gray(&t);
    for radius in [1.5f32, 6.0] {
        let b = ops::blur(&g, radius);
        let ao = ops::ambient_occlusion(&t, radius, 2.0);
        let cu = ops::curvature(&t, radius, 3.0);
        for i in 0..g.px.len() {
            let v = (1.0 - (b.px[i][0] - g.px[i][0]) * 2.0).clamp(0.0, 1.0);
            assert_eq!(ao.px[i], [v, v, v, 1.0]);
            let c = (0.5 + (g.px[i][0] - b.px[i][0]) * 3.0).clamp(0.0, 1.0);
            assert_eq!(cu.px[i], [c, c, c, 1.0]);
        }
    }
}

#[test]
fn blend_writes_every_channel_like_a_copy_of_the_background_would() {
    let fg = Tex::from_fn(20, 16, |u, v| [u, v, 0.3, 0.6]);
    let bg = Tex::from_fn(20, 16, |u, v| [1.0 - u, 0.2, v, 0.8]);
    let mask = Tex::gray_fn(20, 16, |u, _| u);
    for mode in 0..20u32 {
        for am in 0..4u32 {
            let out = ops::blend_ex(&fg, &bg, Some(&mask), mode, 0.7, am, [0.0, 0.0, 1.0, 1.0]);
            // alpha mode 1 keeps the background alpha
            if am == 1 {
                assert!(out.px.iter().zip(&bg.px).all(|(o, b)| o[3] == b[3]), "mode {mode}: alpha");
            }
            assert!(out.px.iter().all(|p| p.iter().all(|c| c.is_finite())));
        }
    }
}

const TRACK: &str = "size 128
node 1 path_track preset=2 gauge=60 rail_width=10 sleeper_spacing=30 sleeper_length=120 sleeper_width=16
node 2 path_cable preset=4 width=40 strands=3
node 3 path_border preset=3 width=30
node 4 path_seam preset=2 stitch_length=16 stitch_gap=8
node 5 path_sample preset=1 width=50
output height 1
output rails 1.rails
output sleepers 1.sleepers
output cable 2
output cmask 2.mask
output strands 2.strand
output border 3
output bmask 3.mask
output seam 4
output stitches 4.stitches
output position 5
output pwidth 5.width
output side 5.side
";

#[test]
fn outputs_shared_by_one_run_equal_outputs_computed_alone() {
    let g = Graph::parse(TRACK).unwrap();
    let all = g.evaluate(None).unwrap();
    // every output on its own: a graph that reads only that output
    for (name, line) in TRACK.lines().filter_map(|l| l.strip_prefix("output ")).map(|l| (l.split(' ').next().unwrap().to_string(), l.to_string())) {
        let body: String = TRACK.lines().filter(|l| !l.starts_with("output ")).collect::<Vec<_>>().join("\n");
        let one = Graph::parse(&format!("{body}\noutput {line}\n")).unwrap().evaluate(None).unwrap();
        assert_eq!(one[&name].px, all[&name].px, "output {name}");
    }
    // and again through the editor's cached passes
    let keep: Vec<u32> = g.output_nodes().iter().map(|o| o.2).collect();
    let cache = Arc::new(NodeCache::new(256));
    for full in [true, false, true] {
        let r = g.evaluate_all_cached(128, &keep, 0, &cache, None, full, &mut |_| {});
        for (u, _, id) in g.output_nodes() {
            assert_eq!(r.kept[&id].px, all[&u].px, "cached output {u} (full {full})");
        }
    }
}

#[test]
fn warm_pass_has_every_thumbnail_and_size_without_recomputing() {
    let mut g = Graph::parse(materialcraft::graph::EXAMPLE).unwrap();
    g.size = 64;
    let keep: Vec<u32> = g.output_nodes().iter().map(|o| o.2).collect();
    let cache = Arc::new(NodeCache::new(256));
    let cold = g.evaluate_all_cached(64, &keep, 0, &cache, None, true, &mut |_| {});
    let misses = cache.stats().1;
    let warm = g.evaluate_all_cached(64, &keep, 0, &cache, None, true, &mut |_| {});
    assert_eq!(cache.stats().1, misses, "nothing may be recomputed");
    assert_eq!(cold.thumbs.len(), g.nodes.len(), "a thumbnail for every node");
    assert_eq!(warm.thumbs.len(), cold.thumbs.len());
    for (id, t) in &cold.thumbs {
        assert_eq!(&warm.thumbs[id], t, "thumbnail of node {id}");
    }
    assert_eq!(warm.sizes, cold.sizes);
}
