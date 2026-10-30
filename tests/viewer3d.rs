// SPDX-License-Identifier: MIT OR Apache-2.0
//! The 3D viewer: environment lighting, light rig, scenes, custom meshes, channels, post-processing and the path tracer.
use materialcraft::export::{self, Format};
use materialcraft::hdri;
use materialcraft::meshio;
use materialcraft::objects::{build_scene, CustomMesh};
use materialcraft::post::{self, PostCtx};
use materialcraft::pt;
use materialcraft::raster;
use materialcraft::render3d::{Maps, View, RELIEF_NONE, SHAPE_CUSTOM};
use materialcraft::scene::{Background, Channel, EnvSettings, Post, SceneKind, ToneMap};
use materialcraft::tex::Tex;

struct M {
    base: Tex,
    normal: Tex,
    rough: Tex,
    metal: Tex,
    ao: Tex,
}

fn mat(base: [f32; 3], rough: f32, metal: f32) -> M {
    M {
        base: Tex::solid(4, 4, [base[0], base[1], base[2], 1.0]),
        normal: Tex::solid(4, 4, [0.5, 0.5, 1.0, 1.0]),
        rough: Tex::solid(4, 4, [rough, rough, rough, 1.0]),
        metal: Tex::solid(4, 4, [metal, metal, metal, 1.0]),
        ao: Tex::solid(4, 4, [1.0; 4]),
    }
}

fn maps(m: &M) -> Maps<'_> {
    Maps { base: &m.base, normal: &m.normal, rough: &m.rough, metal: &m.metal, ao: &m.ao, emissive: None, height: None, opacity: None, env: None }
}

fn plain_view(env: &str) -> View {
    View {
        shape: 0,
        relief: RELIEF_NONE,
        lights: vec![],
        env: EnvSettings { preset: env.into(), background: Background::Color, bg_color: [0.0; 3], ..EnvSettings::default() },
        post: Post { tonemap: ToneMap::Linear, ..Post::default() },
        tess: 64,
        yaw: 0.4,
        pitch: 0.2,
        ..View::default()
    }
}

fn centre_mean(t: &Tex, r: usize) -> [f32; 3] {
    let (cx, cy) = (t.w / 2, t.h / 2);
    let mut a = [0.0f32; 3];
    let mut n = 0.0;
    for y in cy - r..cy + r {
        for x in cx - r..cx + r {
            let p = t.px[y * t.w + x];
            for c in 0..3 {
                a[c] += p[c];
            }
            n += 1.0;
        }
    }
    [a[0] / n, a[1] / n, a[2] / n]
}

fn mean_lum(t: &Tex) -> f32 {
    t.px.iter().map(|p| 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]).sum::<f32>() / t.px.len() as f32
}

#[test]
fn white_furnace_raster() {
    // a white rough ball inside a uniform white environment shows about 1 (the diffuse part plus a little reflection)
    let m = mat([1.0; 3], 1.0, 0.0);
    let v = plain_view("furnace");
    let (fr, _) = raster::render_frame(&maps(&m), &v, 96, 96);
    let c = centre_mean(&fr.hdr, 8);
    for k in 0..3 {
        assert!(c[k] > 0.95 && c[k] < 1.15, "furnace radiance {:?}", c);
    }
}

#[test]
fn white_furnace_path_tracer_conserves_energy() {
    let m = mat([1.0; 3], 1.0, 0.0);
    let v = plain_view("furnace");
    let mp = maps(&m);
    let t = pt::PathTracer::new(&mp, &v, 48, 48);
    let mut st = pt::PtState::new(48, 48);
    t.render_pass(&mut st, 96);
    let img = t.resolve(&st);
    // display values are gamma encoded: undo it
    let c = centre_mean(&img, 5);
    for k in 0..3 {
        let lin = c[k].powf(2.2);
        assert!(lin > 0.9 && lin < 1.08, "path traced furnace radiance {lin} ({:?})", c);
    }
}

#[test]
fn raster_and_path_tracer_agree_on_image_based_lighting() {
    let m = mat([0.5, 0.45, 0.4], 0.6, 0.0);
    let v = plain_view("overcast");
    let mp = maps(&m);
    let (fr, _) = raster::render_frame(&mp, &v, 64, 64);
    let r = centre_mean(&fr.hdr, 6);
    let t = pt::PathTracer::new(&mp, &v, 64, 64);
    let mut st = pt::PtState::new(64, 64);
    t.render_pass(&mut st, 128);
    let img = t.resolve(&st);
    let p = centre_mean(&img, 6);
    for k in 0..3 {
        let pl = p[k].powf(2.2);
        let ratio = pl / r[k].max(1e-4);
        assert!(ratio > 0.82 && ratio < 1.22, "channel {k}: raster {} vs path tracer {pl}", r[k]);
    }
}

#[test]
fn path_tracer_is_deterministic() {
    let m = mat([0.7, 0.3, 0.2], 0.4, 0.0);
    let v = View { scene: SceneKind::Floor, ..plain_view("sky") };
    let mp = maps(&m);
    let a = pt::render(&mp, &v, 32, 32, 4);
    let b = pt::render(&mp, &v, 32, 32, 4);
    assert!(a.px == b.px);
}

#[test]
fn environment_rotation_moves_the_light_not_the_object() {
    let m = mat([0.8; 3], 0.5, 0.0);
    let mut v = plain_view("sunset");
    v.scene = SceneKind::Single;
    let a = raster::render(&maps(&m), &v, 48, 48);
    v.env.rotation = std::f32::consts::PI;
    let b = raster::render(&maps(&m), &v, 48, 48);
    let (la, lb) = (mean_lum(&a), mean_lum(&b));
    // the sunset is very one sided, so turning it around changes the picture a lot
    assert!((la - lb).abs() > 0.01, "rotation had no effect ({la} vs {lb})");
    v.env.rotation = std::f32::consts::TAU;
    let c = raster::render(&maps(&m), &v, 48, 48);
    assert!((mean_lum(&c) - la).abs() < 1e-3, "a full turn must change nothing");
}

#[test]
fn exposure_of_the_environment_scales_the_lighting() {
    let m = mat([0.5; 3], 1.0, 0.0);
    let mut v = plain_view("grey");
    let (a, _) = raster::render_frame(&maps(&m), &v, 40, 40);
    v.env.exposure = 1.0;
    let (b, _) = raster::render_frame(&maps(&m), &v, 40, 40);
    let (ca, cb) = (centre_mean(&a.hdr, 4)[0], centre_mean(&b.hdr, 4)[0]);
    assert!((cb / ca - 2.0).abs() < 0.02, "one stop must double the light ({ca} -> {cb})");
}

#[test]
fn shadows_darken_the_floor() {
    let m = mat([0.8; 3], 0.6, 0.0);
    let mut v = plain_view("night");
    v.scene = SceneKind::Floor;
    v.lights = materialcraft::scene::rig("key_only");
    v.lights[0].follows_camera = false;
    v.lights[0].yaw = 0.3;
    v.lights[0].pitch = 1.1;
    v.lights[0].shadows = true;
    let with = raster::render(&maps(&m), &v, 96, 96);
    v.lights[0].shadows = false;
    let without = raster::render(&maps(&m), &v, 96, 96);
    assert!(mean_lum(&with) < mean_lum(&without) * 0.97, "shadows must take light away ({} vs {})", mean_lum(&with), mean_lum(&without));
}

#[test]
fn a_light_that_is_off_adds_nothing() {
    let m = mat([0.8; 3], 0.5, 0.0);
    let mut v = plain_view("night");
    let a = raster::render(&maps(&m), &v, 40, 40);
    v.lights = materialcraft::scene::rig("three_point");
    for l in v.lights.iter_mut() {
        l.enabled = false;
    }
    let b = raster::render(&maps(&m), &v, 40, 40);
    assert!(a.px == b.px);
}

#[test]
fn channel_views_show_the_raw_maps() {
    let m = mat([0.2, 0.4, 0.6], 0.3, 0.7);
    let mut v = plain_view("studio");
    v.channel = Channel::BaseColor;
    let img = raster::render(&maps(&m), &v, 48, 48);
    let c = centre_mean(&img, 3);
    assert!((c[0] - 0.2).abs() < 0.01 && (c[1] - 0.4).abs() < 0.01 && (c[2] - 0.6).abs() < 0.01, "{c:?}");
    v.channel = Channel::Roughness;
    let c = centre_mean(&raster::render(&maps(&m), &v, 48, 48), 3);
    assert!((c[0] - 0.3).abs() < 0.01);
    v.channel = Channel::Metallic;
    let c = centre_mean(&raster::render(&maps(&m), &v, 48, 48), 3);
    assert!((c[0] - 0.7).abs() < 0.01);
    v.channel = Channel::WorldNormal;
    let c = centre_mean(&raster::render(&maps(&m), &v, 48, 48), 3);
    assert!(c[2] > 0.6, "the ball faces the camera at its centre: {c:?}");
}

#[test]
fn view_settings_round_trip_through_text() {
    let mut v = View::default();
    v.set("light.1.intensity", "2.5").unwrap();
    v.set("env.preset", "sunset").unwrap();
    v.set("env.rotation", "1.25").unwrap();
    v.set("post.bloom", "0.4").unwrap();
    v.set("post.tonemap", "aces").unwrap();
    v.set("channel", "roughness").unwrap();
    v.set("scene", "lookdev").unwrap();
    v.set("camera", "front").unwrap();
    v.set("shape", "torus_knot").unwrap();
    let text = v.to_text();
    let mut w = View::default();
    w.apply_text(&text).unwrap();
    assert_eq!(w.to_text(), text);
    assert!(v.set("nonsense", "1").is_err());
    assert!(v.set("light.99.intensity", "1").is_err());
    assert!(v.set("env.preset", "does_not_exist").is_err());
    assert!(v.set("yaw", "banana").is_err());
}

#[test]
fn camera_presets_orbit_around_the_scene() {
    let mut v = View::default();
    v.set("camera", "top").unwrap();
    let c = v.cam();
    assert!(c.eye[1] > 3.0 && c.fwd[1] < -0.9, "the top camera looks down: {:?}", c.fwd);
    v.set("camera", "front").unwrap();
    let c = v.cam();
    assert!(c.eye[2] > 3.0 && c.eye[0].abs() < 1e-3);
    // distance grows with the scene
    let single = v.cam().dist;
    v.scene = SceneKind::LookDev;
    assert!(v.cam().dist > single * 1.5);
}

#[test]
fn scenes_put_the_floor_under_the_object() {
    for kind in SceneKind::ALL {
        let v = View { scene: kind, tess: 32, ..View::default() };
        let sm = build_scene(&v, None);
        let expect = match kind {
            SceneKind::Single => 1,
            SceneKind::Floor => 2,
            SceneKind::LookDev => 4,
            SceneKind::Pedestal => 4,
        };
        assert_eq!(sm.objects.len(), expect, "{kind:?}");
        assert_eq!(sm.mesh.cull.len(), expect);
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for (ti, t) in sm.mesh.tris.iter().enumerate() {
            let o = sm.mesh.object_of(ti);
            if o == 0 {
                for &i in t {
                    lo = lo.min(sm.mesh.pos[i as usize][1]);
                    hi = hi.max(sm.mesh.pos[i as usize][1]);
                }
            }
        }
        assert!(hi > lo + 1.5, "the main object has a size");
        if kind != SceneKind::Single {
            // the floor plane object (shape 2) lies just under the lowest point of the main object
            let fl = sm.objects.iter().position(|o| o.shape == 2 && o.pitch != 0.0).expect("floor");
            let ys: Vec<f32> = sm.mesh.tris.iter().enumerate().filter(|(ti, _)| sm.mesh.object_of(*ti) == fl).flat_map(|(_, t)| t.iter().map(|&i| sm.mesh.pos[i as usize][1])).collect();
            let fy = ys.iter().sum::<f32>() / ys.len() as f32;
            let expect_y = if kind == SceneKind::Pedestal { lo - 2.4 } else { lo };
            assert!((fy - expect_y).abs() < 0.02, "{kind:?}: floor at {fy}, expected {expect_y}");
        }
    }
}

const CUBE_OBJ: &str = "v -1 -1 -1\nv 1 -1 -1\nv 1 1 -1\nv -1 1 -1\nv -1 -1 1\nv 1 -1 1\nv 1 1 1\nv -1 1 1\n\
f 1 3 2\nf 1 4 3\nf 5 6 7\nf 5 7 8\nf 1 2 6\nf 1 6 5\nf 2 3 7\nf 2 7 6\nf 3 4 8\nf 3 8 7\nf 4 1 5\nf 4 5 8\n";

#[test]
fn custom_meshes_are_normalised_and_get_a_uv_frame() {
    let mesh = meshio::parse_obj(CUBE_OBJ).unwrap();
    let cm = CustomMesh::from_mesh(&mesh).unwrap();
    assert_eq!(cm.tris.len(), 12);
    assert!(cm.generated_uv);
    let far = cm.pos.iter().map(|p| (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt()).fold(0.0f32, f32::max);
    assert!((far - 1.1).abs() < 1e-3, "fits the view: {far}");
    for i in 0..cm.pos.len() {
        let (n, t) = (cm.nrm[i], cm.tan[i]);
        assert!(((n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt() - 1.0).abs() < 1e-3);
        assert!((n[0] * t[0] + n[1] * t[1] + n[2] * t[2]).abs() < 1e-3, "tangent {i} is not perpendicular to the normal");
        assert!(cm.su[i] > 0.0 && cm.sv[i] > 0.0);
        assert_eq!(cm.disp.len(), cm.pos.len());
    }
    // corners share a weld id whichever face they belong to
    let groups = cm.weld.iter().copied().collect::<std::collections::BTreeSet<_>>().len();
    assert_eq!(groups, 8);
    // and it renders
    let m = mat([0.6; 3], 0.5, 0.0);
    let v = View { shape: SHAPE_CUSTOM, custom: Some(std::sync::Arc::new(cm)), ..plain_view("studio") };
    let img = raster::render(&maps(&m), &v, 48, 48);
    assert!(mean_lum(&img) > 0.05);
    assert!(CustomMesh::from_mesh(&meshio::Mesh::default()).is_err());
}

#[test]
fn environment_files_load_and_are_cached() {
    let dir = std::env::temp_dir().join(format!("mc_env_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("sky.hdr");
    let img = Tex::from_fn(64, 32, |_, v| if v < 0.5 { [4.0, 4.0, 4.0, 1.0] } else { [0.1, 0.1, 0.1, 1.0] });
    std::fs::write(&path, export::encode(Format::Hdr, &img, 32, 90)).unwrap();
    let e = hdri::load_env("studio", Some(path.to_str().unwrap())).unwrap();
    assert!(e.background([0.0, 1.0, 0.0])[0] > 3.0 && e.background([0.0, -1.0, 0.0])[0] < 0.3);
    let e2 = hdri::load_env("studio", Some(path.to_str().unwrap())).unwrap();
    assert!(std::sync::Arc::ptr_eq(&e, &e2), "the second load comes from the cache");
    assert!(hdri::load_env("studio", Some(dir.join("missing.hdr").to_str().unwrap())).is_err());
    // an environment image shows up in the background
    let m = mat([0.5; 3], 0.5, 0.0);
    let mut v = plain_view("studio");
    v.env.file = Some(path.to_string_lossy().to_string());
    v.env.background = Background::Environment;
    v.shape = 2;
    v.yaw = 0.0;
    v.pitch = 0.0;
    v.dist = 9.0;
    let img = raster::render(&maps(&m), &v, 40, 40);
    // the top of the picture looks at the horizon and above; the bright half is above the horizon
    assert!(img.px[2 * 40 + 20][0] > img.px[37 * 40 + 20][0], "sky above ground");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn post_processing_chain() {
    let (w, h) = (32, 32);
    let mut hdr = Tex::solid(w, h, [0.2, 0.2, 0.2, 1.0]);
    hdr.px[16 * w + 16] = [40.0, 40.0, 40.0, 1.0];
    let depth = vec![3.0f32; w * h];
    let ctx = PostCtx { tan_half: 0.36, aspect: 1.0, centre_dist: 3.0 };
    let base = Post { tonemap: ToneMap::Linear, ..Post::default() };
    let plain = post::apply(&hdr, &depth, &ctx, &base);
    let glow = post::apply(&hdr, &depth, &ctx, &Post { bloom: 1.0, ..base.clone() });
    assert!(glow.px[16 * w + 19][0] > plain.px[16 * w + 19][0] + 0.02, "bloom spreads light to the neighbours");
    let vig = post::apply(&hdr, &depth, &ctx, &Post { vignette: 1.0, ..base.clone() });
    assert!(vig.px[0][0] < plain.px[0][0] * 0.9 && (vig.px[16 * w + 16][0] - plain.px[16 * w + 16][0]).abs() < 0.05, "vignette darkens the corners only");
    let sat = post::apply(&hdr, &depth, &ctx, &Post { saturation: 0.0, ..base.clone() });
    assert!((sat.px[5][0] - sat.px[5][2]).abs() < 1e-4);
    let warm = post::apply(&hdr, &depth, &ctx, &Post { temperature: 1.0, ..base.clone() });
    assert!(warm.px[5][0] > warm.px[5][2]);
    // ambient occlusion darkens a pixel next to a depth step, leaves flat areas alone
    let mut stepped = vec![3.0f32; w * h];
    for y in 0..h {
        for x in 16..w {
            stepped[y * w + x] = 2.0;
        }
    }
    let flat_hdr = Tex::solid(w, h, [0.5, 0.5, 0.5, 1.0]);
    let ao = post::apply(&flat_hdr, &stepped, &ctx, &Post { ssao: 1.0, ssao_radius: 0.4, ..base.clone() });
    let off = post::apply(&flat_hdr, &stepped, &ctx, &base);
    let near_step = ao.px[16 * w + 14][0];
    assert!(near_step < off.px[16 * w + 14][0] - 0.01, "occluded corner is darker ({near_step} vs {})", off.px[16 * w + 14][0]);
    assert!((ao.px[16 * w + 2][0] - off.px[16 * w + 2][0]).abs() < 0.02, "open areas stay unchanged");
    // depth of field blurs what is out of focus (the largest blur is a fraction of the picture height)
    let n = 128;
    let mut sharp = Tex::solid(n, n, [0.0, 0.0, 0.0, 1.0]);
    for y in 0..n {
        for x in 0..n {
            if (x + y) % 2 == 0 {
                sharp.px[y * n + x] = [1.0, 1.0, 1.0, 1.0];
            }
        }
    }
    let far = vec![9.0f32; n * n];
    let blurred = post::apply(&sharp, &far, &ctx, &Post { dof: 3.0, ..base.clone() });
    let var = |t: &Tex| {
        let m = t.px.iter().map(|p| p[0]).sum::<f32>() / t.px.len() as f32;
        t.px.iter().map(|p| (p[0] - m) * (p[0] - m)).sum::<f32>() / t.px.len() as f32
    };
    assert!(var(&blurred) < var(&post::apply(&sharp, &far, &ctx, &base)) * 0.6, "out of focus means blurred");
}

#[test]
fn tone_mappers_stay_in_range_and_rise() {
    for t in ToneMap::ALL {
        let mut prev = -1.0f32;
        for k in 0..40 {
            let x = 0.01 * 1.25f32.powi(k);
            let y = post::tonemap([x, x, x], t);
            assert!(y[0] >= 0.0 && y[0] <= 1.0001, "{t:?}({x}) = {}", y[0]);
            assert!(y[0] + 1e-4 >= prev, "{t:?} is not monotonic at {x}");
            prev = y[0];
        }
    }
}

#[test]
fn transparent_texels_are_cut_out() {
    // in a white furnace on a black background: an opaque ball shows about 1, a transparent one the background
    let v = plain_view("furnace");
    let opaque = mat([1.0; 3], 1.0, 0.0);
    let mut clear = mat([1.0; 3], 1.0, 0.0);
    clear.base = Tex::solid(4, 4, [1.0, 1.0, 1.0, 0.0]);
    let mut half = mat([1.0; 3], 1.0, 0.0);
    half.base = Tex::solid(4, 4, [1.0, 1.0, 1.0, 0.5]);
    let lum = |c: [f32; 3]| (c[0] + c[1] + c[2]) / 3.0;
    let raster_centre = |m: &Maps| lum(centre_mean(&raster::render_frame(m, &v, 96, 96).0.hdr, 8));
    let full = raster_centre(&maps(&opaque));
    assert!(full > 0.9, "opaque ball {full}");
    assert!(raster_centre(&maps(&clear)) < 0.02, "a zero alpha hides the ball");
    // partial opacity is a dither pattern: about half of the pixels, on average over the area
    let h = raster_centre(&maps(&half));
    assert!(h > full * 0.35 && h < full * 0.65, "half alpha {h} (opaque {full})");
    // an opacity map wins over the base colour's alpha
    let black = Tex::solid(4, 4, [0.0, 0.0, 0.0, 1.0]);
    let white = Tex::solid(4, 4, [1.0; 4]);
    assert!(raster_centre(&Maps { opacity: Some(&black), ..maps(&opaque) }) < 0.02, "a black opacity map hides the ball");
    assert!(raster_centre(&Maps { opacity: Some(&white), ..maps(&clear) }) > 0.9, "a white opacity map shows it");
    // the path tracer lets rays through transparent texels too
    let pt_centre = |m: &Maps| lum(centre_mean(&pt::render(m, &v, 48, 48, 8), 4));
    assert!(pt_centre(&maps(&opaque)) > 0.8);
    assert!(pt_centre(&maps(&clear)) < 0.02, "path tracer: a zero alpha hides the ball");
}
