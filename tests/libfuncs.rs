// SPDX-License-Identifier: MIT OR Apache-2.0
//! The built-in reimplementations of Substance Designer's function presets resolve and compute correctly with no
//! Designer installation and no `functions.sbs` present. Expected values are standard maths, not taken from Adobe.

use materialcraft::func::{FnEnv, Num, Prog, Value};
use materialcraft::libfuncs;
use std::collections::BTreeMap;

fn run(ident: &str, args: &[(&str, Value)]) -> Value {
    let graphs = libfuncs::graphs();
    let name = libfuncs::resolve(ident).unwrap_or_else(|| panic!("'{ident}' is not a built-in library function"));
    let prog = Prog::compile(&graphs[name], graphs).unwrap_or_else(|e| panic!("compile {ident}: {e}"));
    let mut env = FnEnv::new([256.0, 256.0]);
    env.args = args.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
    prog.run(&env, [0.5, 0.5]).unwrap_or_else(|e| panic!("run {ident}: {e}"))
}

fn f(x: f64) -> Value {
    Value::float(x)
}
fn v3(x: f64, y: f64, z: f64) -> Value {
    Value::vec(3, [x, y, z, 0.0])
}
fn approx(got: &Value, want: &[f64]) {
    let Value::Num(Num { lanes, v, .. }) = got else { panic!("not numeric: {got:?}") };
    assert_eq!(*lanes as usize, want.len(), "lane count: got {got:?}, want {want:?}");
    for i in 0..want.len() {
        assert!((v[i] - want[i]).abs() < 1e-4, "lane {i}: got {}, want {} ({got:?})", v[i], want[i]);
    }
}

#[test]
fn built_in_library_functions_compute_the_standard_maths() {
    approx(&run("oneminus", &[("x", f(0.3))]), &[0.7]);
    approx(&run("clamp", &[("input", f(2.0)), ("min", f(0.0)), ("max", f(1.0))]), &[1.0]);
    approx(&run("clamp", &[("input", f(-1.0)), ("min", f(0.0)), ("max", f(1.0))]), &[0.0]);
    approx(&run("saturate", &[("input", f(1.5))]), &[1.0]);
    approx(&run("Pow", &[("x", f(2.0)), ("n", f(3.0))]), &[8.0]);
    approx(&run("Pow", &[("x", f(0.0)), ("n", f(5.0))]), &[0.0]); // Designer's Pow(0, n) is 0
    approx(&run("sign", &[("x", f(0.0))]), &[1.0]); // Designer's sign(0) is 1
    approx(&run("deg_to_rad", &[("deg", f(180.0))]), &[std::f64::consts::PI]);
    approx(&run("ease_in_quad", &[("input", f(0.5))]), &[0.25]);
    approx(&run("ease_out_quad", &[("input", f(0.5))]), &[0.75]);
    // colour: pure red round-trips through HSV
    approx(&run("RGBtoHSV", &[("rgb", v3(1.0, 0.0, 0.0))]), &[0.0, 1.0, 1.0]);
    approx(&run("HSVtoRGB", &[("hsv", v3(0.0, 1.0, 1.0))]), &[1.0, 0.0, 0.0]);
    // green is hue 1/3
    approx(&run("RGB_hue_hexagonal", &[("rgb", v3(0.0, 1.0, 0.0))]), &[1.0 / 3.0]);
    // a 2x2 identity matrix inverts to itself
    approx(&run("matrix_invert", &[("matrix22", Value::vec(4, [1.0, 0.0, 0.0, 1.0]))]), &[1.0, 0.0, 0.0, 1.0]);
}

#[test]
fn a_material_resolves_library_calls_without_the_designer_file() {
    // a package whose own function calls Functions/Math/oneminus, imported with NO functions.sbs available
    std::env::remove_var("MC_SBS_LIBRARY");
    let dir = std::env::temp_dir().join(format!("mc_libfn_nofile_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let main = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><package><identifier v=\"main\"/>\
        <dependencies><dependency><filename v=\"sbs://functions.sbs\"/><uid v=\"55\"/><type v=\"package\"/></dependency></dependencies><content>\
        <function><identifier v=\"call_it\"/><uid v=\"9\"/><paraminputs><paraminput><identifier v=\"t\"/><uid v=\"8\"/><type v=\"256\"/><defaultValue><constantValueFloat1 v=\"0\"/></defaultValue></paraminput></paraminputs>\
        <paramValue><dynamicValue><rootnode v=\"2\"/><paramNodes>\
        <paramNode><uid v=\"1\"/><function v=\"get_float1\"/><funcDatas><funcData><name v=\"get_float1\"/><constantValue><constantValueString v=\"t\"/></constantValue></funcData></funcDatas></paramNode>\
        <paramNode><uid v=\"2\"/><function v=\"instance\"/><connections><connection><identifier v=\"x\"/><connRef v=\"1\"/></connection></connections>\
        <funcDatas><funcData><name v=\"instance\"/><constantValue><constantValueString v=\"pkg:///Functions/Math/oneminus?dependency=55\"/></constantValue></funcData></funcDatas></paramNode>\
        </paramNodes></dynamicValue></paramValue></function>\
        <graph><identifier v=\"g\"/><uid v=\"10\"/><compNodes/></graph></content></package>";
    let path = dir.join("main.sbs");
    std::fs::write(&path, main).unwrap();
    let imported = materialcraft::sbs_import::import_file(&path).unwrap();
    // no "not available" note: the built-in resolved the call
    let missing: Vec<_> = imported.report.notes.iter().filter(|n| n.text.contains("is not available")).map(|n| n.text.clone()).collect();
    assert!(missing.is_empty(), "library call was not resolved without the file: {missing:?}");
    let g = imported.graph.subgraphs.get("call_it").expect("function imported");
    let prog = Prog::compile(g, &imported.graph.subgraphs).unwrap();
    let mut env = FnEnv::new([256.0, 256.0]);
    env.args.insert("t".into(), f(0.25));
    let out = prog.run(&env, [0.5, 0.5]).unwrap();
    approx(&out, &[0.75]); // oneminus(0.25)
    let _ = std::fs::remove_dir_all(&dir);
    let _ = BTreeMap::<u8, u8>::new();
}

fn lanes_of(v: &Value) -> Vec<f64> {
    let Value::Num(Num { lanes, v, .. }) = v else { panic!("not numeric: {v:?}") };
    v[..*lanes as usize].to_vec()
}

#[test]
fn hash_functions_are_deterministic_and_well_spread() {
    let v2 = |x: f64, y: f64| Value::vec(2, [x, y, 0.0, 0.0]);
    let cases: [(&str, usize, usize); 7] = [("hash11", 1, 1), ("hash14", 1, 4), ("hash21", 2, 1), ("hash22", 2, 2), ("hash24", 2, 4), ("hash31", 3, 1), ("hash32", 3, 2)];
    for (ident, n_in, n_out) in cases {
        let seed = |i: usize| -> Value {
            // integer, fractional and negative seeds, a different one per component
            let s = [i as f64 * 0.173 - 300.0, (i % 61) as f64, (i / 61) as f64 * 0.5];
            match n_in {
                1 => f(s[0]),
                2 => v2(s[0], s[1]),
                _ => v3(s[0], s[1], s[2]),
            }
        };
        let mut bins = [0usize; 10];
        let mut same = 0;
        let n = 3000;
        for i in 0..n {
            let out = run(ident, &[("input", seed(i))]);
            assert_eq!(out, run(ident, &[("input", seed(i))]), "{ident} is not deterministic");
            let l = lanes_of(&out);
            assert_eq!(l.len(), n_out, "{ident}: output components");
            for x in &l {
                assert!((0.0..1.0).contains(x), "{ident}: {x} is outside [0, 1)");
                bins[(*x * 10.0) as usize] += 1;
            }
            if l.windows(2).any(|w| w[0] == w[1]) {
                same += 1;
            }
        }
        assert_eq!(same, 0, "{ident}: components should differ");
        // uniform: chi-square of 10 bins (9 degrees of freedom; 30 is far in the tail)
        let total = (n * n_out) as f64;
        let chi: f64 = bins.iter().map(|&b| (b as f64 - total / 10.0).powi(2) / (total / 10.0)).sum();
        assert!(chi < 30.0, "{ident}: uneven spread {bins:?} (chi-square {chi:.1})");
    }
    // nearby seeds land far apart
    let (a, b) = (lanes_of(&run("hash11", &[("input", f(0.5))]))[0], lanes_of(&run("hash11", &[("input", f(0.501))]))[0]);
    assert!((a - b).abs() > 1e-3, "hash11(0.5) = {a}, hash11(0.501) = {b}");
}

#[test]
fn library_functions_are_offered_with_unique_labels() {
    let pal = libfuncs::palette();
    assert!(pal.len() >= 160, "{} library functions offered", pal.len());
    // every one sits in a known folder and has a label of its own, distinct from the atomic nodes'
    let atomic: std::collections::BTreeSet<String> = materialcraft::sbs_defs::palette(materialcraft::sbs_defs::Fam::Function).iter().map(|d| d.label.to_ascii_lowercase()).collect();
    let mut seen = std::collections::BTreeSet::new();
    for e in pal {
        assert!(libfuncs::CATEGORIES.contains(&e.category), "{}: folder {}", e.ident, e.category);
        assert!(seen.insert(e.label.to_ascii_lowercase()), "label '{}' is used twice", e.label);
        assert!(!atomic.contains(&e.label.to_ascii_lowercase()), "label '{}' is also an atomic node's", e.label);
        assert!(e.path.starts_with("Functions/") && e.path.ends_with(e.ident), "{}", e.path);
        assert_eq!(libfuncs::resolve_path(&e.path), libfuncs::resolve(e.ident));
    }
    let find = |ident: &str| pal.iter().find(|e| e.ident == ident).unwrap_or_else(|| panic!("{ident} is not offered"));
    assert_eq!(find("clamp").category, "Math");
    assert_eq!(find("HSVtoRGB").category, "Color");
    assert_eq!(find("hash24").category, "Random");
    assert_eq!(find("hash24").path, "Functions/Random/hash24");
    assert_eq!(find("ease_in_out_cubic").label, "Ease In Out Cubic");
    assert_eq!(find("switch_float2_4_inputs").category, "Switch");
    // the helpers are not offered
    assert!(pal.iter().all(|e| !e.ident.starts_with("mc_")));
    assert!(libfuncs::entry("pkg:///Functions/Math/oneminus?dependency=55").is_some_and(|e| e.ident == "oneminus"));
}

#[test]
fn a_library_call_added_in_the_editor_renders_and_saves_as_the_built_in() {
    use materialcraft::sbs_defs::PTy;
    use materialcraft::sbs_native::{self as native, PVal};
    let e = libfuncs::palette().iter().find(|e| e.ident == "hash31").unwrap();
    // a uniform colour driven by Hash 31 of a constant, the call added as the Library panel adds it (a scalar
    // result: it fills every channel)
    let mut g = native::new_package("libcall", 16);
    g.nodes.insert(1, native::new_node("sbs:uniform", None, [0.0, 0.0]));
    let value = PVal::Const { ty: PTy::F(4), raw: "0 0 0 1".into(), rel: Some("0".into()) };
    let pkg = g.sd.as_ref().unwrap().pkg.clone();
    let name = native::make_function(&mut g.subgraphs, pkg, "libcall", 1, "outputcolor", &value);
    native::set_value(g.nodes.get_mut(&1).unwrap(), "outputcolor", Some(PVal::Dyn { graph: name.clone(), rel: Some("0".into()) }));
    {
        let fg = g.subgraphs.get_mut(&name).unwrap();
        fg.nodes.clear();
        let mut c = native::new_node("sbs:fn:const_float3", None, [0.0, 0.0]);
        native::set_value(&mut c, "const_float3", Some(PVal::Const { ty: PTy::F(3), raw: "0.37 1.5 -2.25".into(), rel: None }));
        fg.nodes.insert(1, c);
        let mut call = native::new_node("sbs:fn:instance", Some(&e.path), [100.0, 0.0]);
        assert_eq!(native::slots(&call, &BTreeMap::new()), ["input"]);
        call.ins.insert("input".into(), 1);
        fg.nodes.insert(2, call);
        fg.sd.as_mut().unwrap().root = Some(2);
        assert_eq!(native::label(&fg.nodes[&2], fg), "Hash 31");
    }
    // the file holds the constant as a 32-bit float
    let want = lanes_of(&run("hash31", &[("input", v3(0.37f32 as f64, 1.5, -2.25))]))[0];
    let check = |g: &materialcraft::graph::Graph, stage: &str| {
        let t = g.evaluate_node(1, 16).unwrap_or_else(|e| panic!("{stage}: {e:?}"));
        for c in 0..4 {
            assert!((t.px[0][c] as f64 - want).abs() < 1e-4, "{stage}: channel {c}: {} vs {want}", t.px[0][c]);
        }
    };
    check(&g, "in the editor");
    // saved as a call to the library package, with the package as a dependency
    let text = native::save_text(&g).unwrap();
    assert!(text.contains("sbs://functions.sbs"), "no dependency on the library package");
    assert!(text.contains("pkg:///Functions/Random/hash31?dependency="), "the call does not name the library function");
    let (back, _) = native::load(&text, None).unwrap();
    check(&back, "after reopening");
    assert_eq!(native::save_text(&back).unwrap(), text, "saving again must not change the file");
}

#[test]
fn is_defined_is_imported() {
    // Is defined of the function's own input (known while importing) and of a variable nothing sets
    std::env::remove_var("MC_SBS_LIBRARY");
    let dir = std::env::temp_dir().join(format!("mc_isdefined_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let func = |id: &str, uid: u32, var: &str| {
        format!(
            "<function><identifier v=\"{id}\"/><uid v=\"{uid}\"/><paraminputs><paraminput><identifier v=\"t\"/><uid v=\"{}\"/><type v=\"256\"/><defaultValue><constantValueFloat1 v=\"0\"/></defaultValue></paraminput></paraminputs>\
            <paramValue><dynamicValue><rootnode v=\"1\"/><paramNodes>\
            <paramNode><uid v=\"1\"/><function v=\"isdefined\"/><funcDatas><funcData><name v=\"isdefined\"/><constantValue><constantValueString v=\"{var}\"/></constantValue></funcData></funcDatas></paramNode>\
            </paramNodes></dynamicValue></paramValue></function>",
            uid + 1
        )
    };
    let main = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><package><identifier v=\"main\"/><content>{}{}<graph><identifier v=\"g\"/><uid v=\"10\"/><compNodes/></graph></content></package>",
        func("has_t", 20, "t"),
        func("has_nope", 30, "nope")
    );
    let path = dir.join("main.sbs");
    std::fs::write(&path, main).unwrap();
    let imported = materialcraft::sbs_import::import_file(&path).unwrap();
    for (name, want) in [("has_t", true), ("has_nope", false)] {
        let g = imported.graph.subgraphs.get(name).unwrap_or_else(|| panic!("{name} imported"));
        let prog = Prog::compile(g, &imported.graph.subgraphs).unwrap();
        assert_eq!(prog.run(&FnEnv::new([256.0, 256.0]), [0.5, 0.5]).unwrap(), Value::Bool(want), "{name}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// ------------------------------------------------------------------------------------------------ SDF functions

const EPS: f64 = 1e-4;

/// A shape at `p`, given as its `P` input, with the other inputs as listed and their defaults otherwise.
fn dist(ident: &str, args: &[(&str, Value)], p: [f64; 3]) -> f64 {
    let mut a = args.to_vec();
    a.push(("P", v3(p[0], p[1], p[2])));
    lanes_of(&run(ident, &a))[0]
}

fn near(ident: &str, args: &[(&str, Value)], p: [f64; 3], want: f64) {
    let got = dist(ident, args, p);
    assert!((got - want).abs() < EPS, "{ident} at {p:?}: got {got}, want {want}");
}

#[test]
fn sdf_primitives_measure_the_known_distances() {
    let v2 = |x: f64, y: f64| Value::vec(2, [x, y, 0.0, 0.0]);
    let at0 = ("center_position", v3(0.0, 0.0, 0.0));
    let s = 0.5 * 2f64.sqrt();

    // inside negative, on the surface 0, outside positive, on the defaults of the reference
    near("3d_sdf_sphere", &[], [0.0, 0.0, 0.0], -0.5);
    near("3d_sdf_sphere", &[], [0.5, 0.0, 0.0], 0.0);
    near("3d_sdf_sphere", &[], [1.5, 0.0, 0.0], 1.0);
    near("3d_sdf_sphere", &[("radius", f(1.0)), ("center_position", v3(0.0, 2.0, 0.0))], [0.0, 4.0, 0.0], 1.0);

    near("3d_sdf_torus", &[], [0.5, 0.0, 0.0], -0.2);
    near("3d_sdf_torus", &[], [0.7, 0.0, 0.0], 0.0);
    near("3d_sdf_torus", &[], [0.0, 0.0, 0.0], 0.3); // the hole
    near("3d_sdf_torus", &[], [0.5, 0.0, 1.2], 1.0);

    near("3d_sdf_cone", &[], [0.0, 0.0, 0.25], -0.25); // nearest the base
    near("3d_sdf_cone", &[], [0.25, 0.0, 0.5], 0.0); // on the slope
    near("3d_sdf_cone", &[], [0.0, 0.0, 0.0], 0.0); // on the base
    near("3d_sdf_cone", &[], [0.0, 0.0, 2.0], 1.0); // above the apex
    near("3d_sdf_cone", &[], [0.0, 0.0, -1.0], 1.0);

    let slant = (0.5 - 0.5 * 0.3) / 1.09f64.sqrt();
    near("3d_sdf_capped_cone", &[], [0.0, 0.0, 0.5], -slant);
    near("3d_sdf_capped_cone", &[], [0.35, 0.0, 0.5], 0.0);
    near("3d_sdf_capped_cone", &[], [0.0, 0.0, 2.0], 1.0);

    // a slanted axis of length 5
    let ends = [("position_base", v3(0.0, 0.0, 0.0)), ("position_top", v3(0.0, 3.0, 4.0)), ("radius_base", f(0.5)), ("radius_top", f(0.5))];
    near("3d_sdf_capped_cone_2_points", &ends, [0.0, 1.5, 2.0], -0.5);
    near("3d_sdf_capped_cone_2_points", &ends, [0.5, 1.5, 2.0], 0.0);
    near("3d_sdf_capped_cone_2_points", &ends, [0.0, 4.8, 6.4], 3.0);
    near("3d_sdf_capped_cone_2_points", &[], [0.0, 0.0, 0.1], -0.1); // nearest the base

    // trimmed around +X; the arc that is left ends at +/-135 degrees
    let torus = [at0.clone()];
    near("3d_sdf_capped_torus", &torus, [-0.5, 0.0, 0.0], -0.2);
    near("3d_sdf_capped_torus", &torus, [-0.7, 0.0, 0.0], 0.0);
    let tip = 0.5 * std::f64::consts::FRAC_1_SQRT_2;
    near("3d_sdf_capped_torus", &torus, [1.0, 0.0, 0.0], ((1.0 + tip).powi(2) + tip * tip).sqrt() - 0.2);
    // trimmed one way, from +X through +Y to -X
    let oneway = [at0.clone(), ("angle", f(0.5)), ("symmetrical", Value::Bool(false))];
    near("3d_sdf_capped_torus", &oneway, [0.0, -0.5, 0.0], -0.2);
    near("3d_sdf_capped_torus", &oneway, [0.0, 0.5, 0.0], s - 0.2);
    // the offset turns the trimmed arc: a half turn moves it to -X
    let turned = [at0.clone(), ("angle_offset", f(0.5))];
    near("3d_sdf_capped_torus", &turned, [0.5, 0.0, 0.0], -0.2);

    near("3d_sdf_capsule", &[], [0.0, 0.0, 0.5], -0.25);
    near("3d_sdf_capsule", &[], [0.25, 0.0, 0.5], 0.0);
    near("3d_sdf_capsule", &[], [0.0, 0.0, 2.25], 1.0);
    let tips = [("start_end_at_tip", Value::Bool(true))];
    near("3d_sdf_capsule", &tips, [0.0, 0.0, 1.0], 0.0); // the tip is at End
    near("3d_sdf_capsule", &tips, [0.0, 0.0, 0.5], -0.25);

    // the default pivot stands the cube on the ground
    near("3d_sdf_cube", &[], [0.0, 0.0, 0.5], -0.5);
    near("3d_sdf_cube", &[], [0.0, 0.0, 1.0], 0.0);
    near("3d_sdf_cube", &[], [0.0, 0.0, 2.0], 1.0);
    near("3d_sdf_cube", &[], [1.5, 1.5, 1.5], 1.5);
    let centred = [("pivot_position_local", v3(0.0, 0.0, 0.0))];
    near("3d_sdf_cube", &centred, [0.0, 0.0, 0.0], -0.5);
    let round = [("rounding", f(0.1))];
    near("3d_sdf_cube", &round, [0.5, 0.0, 0.5], 0.0); // rounding keeps the size
    near("3d_sdf_cube", &round, [0.5, 0.5, 1.0], 3f64.sqrt() * 0.1 - 0.1);

    near("3d_sdf_cylinder", &[], [0.0, 0.0, 0.5], -0.5);
    near("3d_sdf_cylinder", &[], [0.5, 0.0, 0.5], 0.0);
    near("3d_sdf_cylinder", &[], [1.5, 0.0, 0.5], 1.0);
    near("3d_sdf_cylinder", &[], [0.0, 0.0, 2.0], 1.0);
    near("3d_sdf_cylinder", &round, [0.5, 0.0, 1.0], 2f64.sqrt() * 0.1 - 0.1);

    near("3d_sdf_cylinder_2_points", &[], [0.0, 0.0, 0.5], -0.25);
    near("3d_sdf_cylinder_2_points", &[], [0.25, 0.0, 0.5], 0.0);
    near("3d_sdf_cylinder_2_points", &[], [1.25, 0.0, 0.5], 1.0);
    near("3d_sdf_cylinder_2_points", &[], [0.0, 0.0, 2.0], 1.0);

    near("3d_sdf_ellipsoid", &[], [0.0, 0.0, 0.0], -0.35);
    near("3d_sdf_ellipsoid", &[], [0.35, 0.0, 0.0], 0.0);
    near("3d_sdf_ellipsoid", &[], [0.0, 0.0, 0.5], 0.0);
    near("3d_sdf_ellipsoid", &[], [0.0, 0.0, 1.5], 1.0);

    // two discs of radius 0.5, 0.5 apart along X, from z = 0 to z = 0.5
    near("3d_sdf_elongated_cylinder", &[], [0.0, 0.0, 0.25], -0.25);
    near("3d_sdf_elongated_cylinder", &[], [0.75, 0.0, 0.25], 0.0);
    near("3d_sdf_elongated_cylinder", &[], [0.0, 0.5, 0.25], 0.0);
    near("3d_sdf_elongated_cylinder", &[], [1.75, 0.0, 0.25], 1.0);

    let g = [("height", f(1.0))];
    near("3d_sdf_ground_plane", &g, [0.0, 0.0, 0.0], -1.0);
    near("3d_sdf_ground_plane", &g, [5.0, -3.0, 1.0], 0.0);
    near("3d_sdf_ground_plane", &g, [0.0, 0.0, 3.0], 2.0);

    let ip = [("normal", v3(0.0, 0.0, 2.0)), ("center_position", f(0.5))]; // the normal is normalized
    near("3d_sdf_infinite_plane", &ip, [9.0, 9.0, 0.0], -0.5);
    near("3d_sdf_infinite_plane", &ip, [9.0, 9.0, 0.5], 0.0);
    near("3d_sdf_infinite_plane", &ip, [9.0, 9.0, 3.0], 2.5);
    near("3d_sdf_infinite_plane", &[("normal", v3(1.0, 1.0, 0.0))], [1.0, 1.0, 0.0], 2f64.sqrt());

    let pl = [("thickness", f(0.1))];
    near("3d_sdf_plane", &pl, [0.0, 0.0, 0.0], -0.1);
    near("3d_sdf_plane", &pl, [0.3, 0.3, 0.1], 0.0);
    near("3d_sdf_plane", &pl, [0.0, 0.0, 1.1], 1.0);
    near("3d_sdf_plane", &pl, [1.0, 0.0, 0.0], 0.4); // past its edge
    near("3d_sdf_plane", &[], [0.2, 0.2, 0.0], 0.0); // no thickness: only a surface
    // turned to face +X: the size lies in Y and Z
    let side = [("thickness", f(0.1)), ("normal", v3(1.0, 0.0, 0.0))];
    near("3d_sdf_plane", &side, [0.1, 0.3, 0.3], 0.0);
    near("3d_sdf_plane", &side, [0.0, 1.0, 0.0], 0.4);

    near("3d_sdf_pyramid", &[], [0.0, 0.0, 0.25], -0.25);
    near("3d_sdf_pyramid", &[], [0.25, 0.0, 0.5], 0.0); // on a face
    near("3d_sdf_pyramid", &[], [0.0, 0.0, 0.0], 0.0);
    near("3d_sdf_pyramid", &[], [0.0, 0.0, 2.0], 1.0); // above the apex
    near("3d_sdf_pyramid", &[], [0.0, 0.0, -1.0], 1.0);
    near("3d_sdf_pyramid", &[], [1.5, 0.0, 0.0], 1.0); // beside the base
    near("3d_sdf_pyramid", &[], [0.5 + 2.0 / 5f64.sqrt(), 0.0, 1.0 / 5f64.sqrt()], 1.0); // straight off a base edge
    near("3d_sdf_pyramid", &[("base_size", v2(2.0, 1.0))], [1.0, 0.0, 0.0], 0.0);
    near("3d_sdf_pyramid", &[("base_size", v2(2.0, 1.0))], [0.0, 0.5, 0.0], 0.0);
    let (bs, up) = (("base_size", f(2.0)), ("base_position", v3(0.0, 0.0, 1.0)));
    near("3d_sdf_pyramid_square", &[bs.clone(), up.clone()], [0.5, 0.5, 1.5], 0.0);
    near("3d_sdf_pyramid_square", &[bs.clone(), up.clone()], [0.0, 0.0, 1.0], 0.0);
    near("3d_sdf_pyramid_square", &[bs.clone(), up.clone()], [0.0, 0.0, 3.0], 1.0);
    assert!(dist("3d_sdf_pyramid_square", &[bs, up], [0.0, 0.0, 1.2]) < 0.0);

    near("3d_sdf_helix", &[], [0.4, 0.0, 0.0], -0.1); // on the curve, where it starts
    near("3d_sdf_helix", &[], [0.5, 0.0, 0.0], 0.0);
    near("3d_sdf_helix", &[], [1.4, 0.0, 0.0], 0.9);
    near("3d_sdf_helix", &[], [0.4, 0.0, 0.125], -0.1); // one turn up
    near("3d_sdf_helix", &[], [-0.4, 0.0, 0.0625], -0.1); // half a turn: half a coil up
    near("3d_sdf_helix", &[], [0.0, 0.0, 0.25], 0.3); // on the axis
    assert!(dist("3d_sdf_helix", &[], [0.4, 0.0, 2.0]) > 1.0); // above the last turn
    assert!(dist("3d_sdf_helix", &[], [0.4, 0.0, -2.0]) > 1.0);

    near("3d_sdf_hexagonal_prism", &[], [0.0, 0.0, 0.5], -0.5 * 0.8660254);
    near("3d_sdf_hexagonal_prism", &[], [0.5, 0.0, 0.5], 0.0); // a corner
    near("3d_sdf_hexagonal_prism", &[], [0.0, 0.5 * 0.8660254, 0.5], 0.0); // the middle of a side
    near("3d_sdf_hexagonal_prism", &[], [0.0, 1.0 + 0.5 * 0.8660254, 0.5], 1.0);
    near("3d_sdf_hexagonal_prism", &[], [1.5, 0.0, 0.5], 1.0);
    near("3d_sdf_hexagonal_prism", &[], [0.0, 0.0, 2.0], 1.0);
}

#[test]
fn sdf_rock_is_closed_faceted_and_seeded() {
    let mid = |d: [f64; 3], t: f64| [d[0] * t, d[1] * t, 0.5 + d[2] * t];
    // the first facet's direction
    let (z, r) = (0.875f64, (1.0 - 0.875f64 * 0.875).sqrt());
    let facet = [r, 0.0, z];
    near("3d_sdf_rock", &[], mid(facet, 0.0), -0.5);
    near("3d_sdf_rock", &[], mid(facet, 0.5), 0.0);
    near("3d_sdf_rock", &[], mid(facet, 3.0), 2.5);
    // closed in every direction, however few facets are asked for
    for facets in [1, 4, 8, 32, 99] {
        let args = [("max_facets", Value::int(facets))];
        for i in 0..200 {
            let (a, b) = (i as f64 * 2.399963, 1.0 - (2 * i + 1) as f64 / 200.0);
            let c = (1.0 - b * b).sqrt();
            let d = dist("3d_sdf_rock", &args, mid([c * a.cos(), c * a.sin(), b], 20.0));
            assert!(d > 0.0, "{facets} facets: open at direction {i} ({d})");
        }
    }
    let (some, many) = ([("max_facets", Value::int(8))], [("max_facets", Value::int(32))]);
    assert_ne!(dist("3d_sdf_rock", &some, [0.3, 0.3, 0.9]), dist("3d_sdf_rock", &many, [0.3, 0.3, 0.9]));
    // the seed changes a random rock and nothing else; randomness makes it smaller
    let rand = |seed: f64| [("randomness", f(0.4)), ("seed", f(seed))];
    assert_eq!(dist("3d_sdf_rock", &rand(1.0), facet), dist("3d_sdf_rock", &rand(1.0), facet));
    assert_ne!(dist("3d_sdf_rock", &rand(1.0), facet), dist("3d_sdf_rock", &rand(2.0), facet));
    assert_eq!(dist("3d_sdf_rock", &[("seed", f(1.0))], facet), dist("3d_sdf_rock", &[("seed", f(2.0))], facet));
    assert!(dist("3d_sdf_rock", &rand(1.0), [0.0, 0.0, 0.5]) > -0.5, "randomness pulls the faces in");
    // smoothness rounds: where two faces meet, the blend is further out than their corner
    let d = |k: f64| dist("3d_sdf_rock", &[("smoothness", f(k))], [0.0, 0.0, 1.0]);
    assert!(d(0.2) > d(0.0), "{} vs {}", d(0.2), d(0.0));
}

#[test]
fn sdf_operators_combine_distances() {
    let two = |a: f64, b: f64| [("sdf_1", f(a)), ("sdf_2", f(b))];
    let sm = |a: f64, b: f64, k: f64| [("sdf_1", f(a)), ("sdf_2", f(b)), ("smoothness", f(k))];
    let is = |ident: &str, args: &[(&str, Value)], want: f64| {
        let got = lanes_of(&run(ident, args))[0];
        assert!((got - want).abs() < EPS, "{ident} {args:?}: got {got}, want {want}");
    };
    is("3d_sdf_op_union", &two(0.3, -0.1), -0.1);
    is("3d_sdf_op_intersection", &two(0.3, -0.1), 0.3);
    // SDF 1 is what is cut, SDF 2 what cuts it
    is("3d_sdf_op_subtraction", &two(-0.5, 0.2), -0.2); // inside, nearer the cut than its own surface
    is("3d_sdf_op_subtraction", &two(-0.5, -0.2), 0.2); // in the cut: outside
    is("3d_sdf_op_subtraction", &two(0.3, 0.9), 0.3);

    // where both are equal the blend moves by a quarter of the smoothness; far from there, the plain result
    is("3d_sdf_op_union_smooth", &sm(0.1, 0.1, 0.4), 0.0);
    is("3d_sdf_op_union_smooth", &sm(1.0, -1.0, 0.4), -1.0);
    is("3d_sdf_op_union_smooth", &sm(0.3, -0.1, 0.0), -0.1);
    is("3d_sdf_op_intersection_smooth", &sm(0.1, 0.1, 0.4), 0.2);
    is("3d_sdf_op_intersection_smooth", &sm(1.0, -1.0, 0.4), 1.0);
    is("3d_sdf_op_intersection_smooth", &sm(0.3, -0.1, 0.0), 0.3);
    is("3d_sdf_op_subtraction_smooth", &sm(0.1, -0.1, 0.4), 0.2);
    is("3d_sdf_op_subtraction_smooth", &sm(-1.0, 2.0, 0.4), -1.0);
    is("3d_sdf_op_subtraction_smooth", &sm(-0.5, -0.2, 0.0), 0.2);

    let ch = |a: f64, b: f64, r: f64| [("sdf_1", f(a)), ("sdf_2", f(b)), ("radius", f(r))];
    is("3d_sdf_op_union_chamfer", &ch(0.1, 0.1, 0.1), 0.1 * std::f64::consts::FRAC_1_SQRT_2);
    is("3d_sdf_op_union_chamfer", &ch(0.1, 0.1, 0.0), 0.1);
    is("3d_sdf_op_union_chamfer", &ch(2.0, -1.0, 0.1), -1.0);

    let surf = |a: f64, b: f64| [("base_sdf", f(a)), ("intersecting_sdf", f(b))];
    is("3d_sdf_op_intersection_surface", &surf(0.01, -1.0), -0.01);
    is("3d_sdf_op_intersection_surface", &surf(0.5, -1.0), 0.48);
    is("3d_sdf_op_intersection_surface", &surf(0.0, 0.3), 0.3);

    is("3d_sdf_op_morph", &[("base_sdf", f(0.2)), ("target_sdf", f(0.6)), ("mix_factor", f(0.25))], 0.3);
    is("3d_sdf_op_rounding", &[("sdf", f(0.2))], 0.15);
    is("3d_sdf_op_shell", &[("sdf", f(-0.3))], 0.28);
    is("3d_sdf_op_shell", &[("sdf", f(0.01))], -0.01);
}

/// A function graph made of calls, as an import or the editor makes one.
struct Scene {
    g: materialcraft::graph::Graph,
    lib: BTreeMap<String, materialcraft::graph::Graph>,
    next: u32,
}

impl Scene {
    fn new() -> Scene {
        let mut g = materialcraft::graph::Graph::default();
        g.meta.insert("kind".into(), "function".into());
        Scene { g, lib: libfuncs::graphs().clone(), next: 1 }
    }
    fn add(&mut self, n: materialcraft::graph::Node) -> u32 {
        self.g.nodes.insert(self.next, n);
        self.next += 1;
        self.next - 1
    }
    fn konst(&mut self, kind: &str, nums: &[(&str, f32)]) -> u32 {
        let mut n = materialcraft::graph::Node { kind: kind.into(), ..Default::default() };
        n.nums.extend(nums.iter().map(|(k, v)| (k.to_string(), *v)));
        self.add(n)
    }
    fn float(&mut self, v: f32) -> u32 {
        self.konst("f_float", &[("v", v)])
    }
    fn int(&mut self, v: f32) -> u32 {
        self.konst("f_int", &[("v", v)])
    }
    fn vec3(&mut self, x: f32, y: f32, z: f32) -> u32 {
        self.konst("f_vec3", &[("x", x), ("y", y), ("z", z)])
    }
    fn ivec3(&mut self, x: f32, y: f32, z: f32) -> u32 {
        self.konst("f_ivec3", &[("x", x), ("y", y), ("z", z)])
    }
    /// A call to a library function, its inputs named.
    fn call(&mut self, ident: &str, ins: &[(&str, u32)]) -> u32 {
        let e = libfuncs::palette().iter().find(|e| e.ident == ident).unwrap_or_else(|| panic!("{ident}"));
        let mut n = materialcraft::graph::Node { kind: "f_call".into(), ..Default::default() };
        n.strs.insert("fn".into(), libfuncs::resolve(ident).unwrap().into());
        for (k, v) in ins {
            let pos = e.inputs.iter().position(|(id, _)| id == k).unwrap_or_else(|| panic!("{ident} has no input {k}"));
            n.ins.insert(materialcraft::func::CALL_SLOTS[pos].into(), *v);
        }
        self.add(n)
    }
    /// A sphere of radius `r` around `c`.
    fn sphere(&mut self, r: f32, c: [f32; 3]) -> u32 {
        let (radius, center) = (self.float(r), self.vec3(c[0], c[1], c[2]));
        self.call("3d_sdf_sphere", &[("radius", radius), ("center_position", center)])
    }
    fn set_out(&mut self, root: u32) {
        let mut o = materialcraft::graph::Node { kind: "f_out".into(), ..Default::default() };
        o.ins.insert("in".into(), root);
        self.add(o);
    }
    fn out(mut self, root: u32) -> Prog {
        self.set_out(root);
        Prog::compile(&self.g, &self.lib).unwrap()
    }
}

fn at(prog: &Prog, p: [f64; 3], want: f64, what: &str) {
    let (v, _) = prog.run_sdf(&FnEnv::new([256.0, 256.0]), p).unwrap();
    let got = lanes_of(&v)[0];
    assert!((got - want).abs() < EPS, "{what} at {p:?}: got {got}, want {want}");
}

#[test]
fn sdf_primitives_measure_from_the_space_they_are_evaluated_in() {
    // P not connected: the host's position
    let mut s = Scene::new();
    let n = s.sphere(0.5, [1.0, 0.0, 0.0]);
    let prog = s.out(n);
    at(&prog, [1.0, 0.0, 0.0], -0.5, "world position");
    at(&prog, [1.0, 0.0, 2.5], 2.0, "world position");
    // no host: the pixel, on the ground plane
    approx(&prog.run(&FnEnv::new([256.0, 256.0]), [1.0, 0.0]).unwrap(), &[-0.5]);
}

#[test]
fn sdf_transforms_move_the_shape_connected_to_them() {
    // an Offset of a shape made before it: what an expression over a computed distance could not do
    let mut s = Scene::new();
    let (shape, off) = (s.sphere(0.5, [0.0; 3]), s.vec3(0.0, 0.0, 2.0));
    let n = s.call("3d_sdf_transform_offset", &[("sdf", shape), ("offset", off)]);
    let offset = s.out(n);
    at(&offset, [0.0, 0.0, 2.0], -0.5, "offset");
    at(&offset, [0.0, 0.0, 2.5], 0.0, "offset");
    at(&offset, [0.0, 0.0, 0.0], 1.5, "offset");

    // the same through P: one space for what it is connected to
    let mut s = Scene::new();
    let o = s.vec3(0.0, 0.0, 2.0);
    let p = s.call("3d_sdf_transform_offset_p", &[("offset", o)]);
    let n = s.call("3d_sdf_sphere", &[("P", p)]);
    let offset_p = s.out(n);
    for q in [[0.0, 0.0, 2.0], [0.3, 0.1, 0.7], [-2.0, 1.0, 3.0]] {
        let d = lanes_of(&offset.run_sdf(&FnEnv::new([256.0, 256.0]), q).unwrap().0)[0];
        at(&offset_p, q, d, "offset P");
    }

    // a quarter turn around Z takes +X to +Y (the axis is normalized)
    let mut s = Scene::new();
    let (shape, angle, axis) = (s.sphere(0.5, [1.0, 0.0, 0.0]), s.float(0.25), s.vec3(0.0, 0.0, 3.0));
    let n = s.call("3d_sdf_transform_rotate", &[("sdf", shape), ("angle", angle), ("axis", axis)]);
    let rotate = s.out(n);
    at(&rotate, [0.0, 1.0, 0.0], -0.5, "rotate");
    at(&rotate, [0.0, 1.5, 0.0], 0.0, "rotate");
    at(&rotate, [1.0, 0.0, 0.0], 2f64.sqrt() - 0.5, "rotate");

    // and the pivot stays where it is
    let mut s = Scene::new();
    let (shape, angle, axis, pivot) = (s.sphere(0.5, [1.0, 0.0, 0.0]), s.float(0.5), s.vec3(0.0, 0.0, 1.0), s.vec3(2.0, 0.0, 0.0));
    let n = s.call("3d_sdf_transform_rotate", &[("sdf", shape), ("angle", angle), ("axis", axis), ("pivot_position", pivot)]);
    at(&s.out(n), [3.0, 0.0, 0.0], -0.5, "rotate around a pivot");

    let mut s = Scene::new();
    let (angle, axis) = (s.float(0.25), s.vec3(0.0, 0.0, 1.0));
    let p = s.call("3d_sdf_transform_rotate_p", &[("angle", angle), ("axis", axis)]);
    let (r, c) = (s.float(0.5), s.vec3(1.0, 0.0, 0.0));
    let n = s.call("3d_sdf_sphere", &[("radius", r), ("center_position", c), ("P", p)]);
    let rotate_p = s.out(n);
    at(&rotate_p, [0.0, 1.0, 0.0], -0.5, "rotate P");
    at(&rotate_p, [0.0, 1.5, 0.0], 0.0, "rotate P");

    // the distance scales with the shape
    let mut s = Scene::new();
    let (shape, k) = (s.sphere(0.5, [0.0; 3]), s.float(2.0));
    let n = s.call("3d_sdf_transform_scale", &[("sdf", shape), ("scale", k)]);
    let scale = s.out(n);
    at(&scale, [0.0, 0.0, 0.0], -1.0, "scale");
    at(&scale, [1.0, 0.0, 0.0], 0.0, "scale");
    at(&scale, [3.0, 0.0, 0.0], 2.0, "scale");

    let mut s = Scene::new();
    let shape = s.sphere(0.5, [1.0, 0.0, 0.0]);
    let n = s.call("3d_sdf_transform_flip", &[("sdf", shape)]); // X by default
    let flip = s.out(n);
    at(&flip, [-1.0, 0.0, 0.0], -0.5, "flip");
    at(&flip, [1.0, 0.0, 0.0], 1.5, "flip");

    // a sphere stretched by 1 along Z is a capsule: the slice at the centre runs on, the top moves up
    let mut s = Scene::new();
    let (shape, e) = (s.sphere(0.5, [0.0; 3]), s.vec3(0.0, 0.0, 1.0));
    let n = s.call("3d_sdf_transform_elongate", &[("sdf", shape), ("elongation", e)]);
    let elongate = s.out(n);
    at(&elongate, [0.0, 0.0, 0.75], -0.5, "elongate");
    at(&elongate, [0.0, 0.0, 1.5], 0.0, "elongate");
    at(&elongate, [0.0, 0.0, -0.5], 0.0, "elongate");
    at(&elongate, [0.5, 0.0, 0.5], 0.0, "elongate");

    // a quarter turn around Z, spread over z = -1..1: none below, all above, half of it half way
    let twist = |c: [f32; 3]| {
        let mut s = Scene::new();
        let (shape, a, b, e) = (s.sphere(0.25, c), s.float(0.25), s.float(-1.0), s.float(1.0));
        let n = s.call("3d_sdf_transform_twist", &[("sdf", shape), ("angle", a), ("start", b), ("end", e)]);
        s.out(n)
    };
    let c = std::f64::consts::FRAC_1_SQRT_2;
    at(&twist([1.0, 0.0, 0.0]), [c, c, 0.0], -0.25, "twist half way");
    at(&twist([1.0, 0.0, -2.0]), [1.0, 0.0, -2.0], -0.25, "twist below the start");
    at(&twist([1.0, 0.0, 2.0]), [0.0, 1.0, 2.0], -0.25, "twist above the end");

    // around Y from z = 0 to z = 1
    let bend = |c: [f32; 3]| {
        let mut s = Scene::new();
        let (shape, a, b, e) = (s.sphere(0.25, c), s.float(0.05), s.float(0.0), s.float(1.0));
        let n = s.call("3d_sdf_transform_bend", &[("sdf", shape), ("angle", a), ("start", b), ("end", e)]);
        s.out(n)
    };
    at(&bend([0.0, 0.0, -2.0]), [0.0, 0.0, -2.0], -0.25, "bend below the start");
    let t = 0.05 * std::f64::consts::TAU;
    at(&bend([0.0, 0.0, 2.0]), [2.0 * t.sin(), 0.0, 2.0 * t.cos()], -0.25, "bend above the end");
}

#[test]
fn sdf_transforms_chain_and_a_shared_shape_stays_itself() {
    // Offset of a Rotate: turned first, then moved
    let mut s = Scene::new();
    let (shape, angle, axis, off) = (s.sphere(0.5, [1.0, 0.0, 0.0]), s.float(0.25), s.vec3(0.0, 0.0, 1.0), s.vec3(0.0, 0.0, 3.0));
    let r = s.call("3d_sdf_transform_rotate", &[("sdf", shape), ("angle", angle), ("axis", axis)]);
    let n = s.call("3d_sdf_transform_offset", &[("sdf", r), ("offset", off)]);
    let chain = s.out(n);
    at(&chain, [0.0, 1.0, 3.0], -0.5, "chain");
    at(&chain, [0.0, 1.0, 0.0], 2.5, "chain");

    // one shape, two offsets: two shapes, and the shape's own place still counts
    let mut s = Scene::new();
    let (shape, l, r) = (s.sphere(0.5, [0.0; 3]), s.vec3(-2.0, 0.0, 0.0), s.vec3(2.0, 0.0, 0.0));
    let a = s.call("3d_sdf_transform_offset", &[("sdf", shape), ("offset", l)]);
    let b = s.call("3d_sdf_transform_offset", &[("sdf", shape), ("offset", r)]);
    let u = s.call("3d_sdf_op_union", &[("sdf_1", a), ("sdf_2", b)]);
    let n = s.call("3d_sdf_op_union", &[("sdf_1", u), ("sdf_2", shape)]);
    let copies = s.out(n);
    for x in [-2.0, 0.0, 2.0] {
        at(&copies, [x, 0.0, 0.0], -0.5, "copies");
    }
    at(&copies, [1.0, 0.0, 0.0], 0.5, "copies");

    // a transform of an operator moves all of it
    let mut s = Scene::new();
    let (a, b, off) = (s.sphere(0.5, [0.0; 3]), s.sphere(0.5, [1.0, 0.0, 0.0]), s.vec3(0.0, 4.0, 0.0));
    let u = s.call("3d_sdf_op_union", &[("sdf_1", a), ("sdf_2", b)]);
    let n = s.call("3d_sdf_transform_offset", &[("sdf", u), ("offset", off)]);
    let moved = s.out(n);
    at(&moved, [0.0, 4.0, 0.0], -0.5, "moved union");
    at(&moved, [1.0, 4.0, 0.0], -0.5, "moved union");

    // an operator after a transform works on the moved shape
    let mut s = Scene::new();
    let (shape, off, r) = (s.sphere(0.5, [0.0; 3]), s.vec3(1.0, 0.0, 0.0), s.float(0.25));
    let m = s.call("3d_sdf_transform_offset", &[("sdf", shape), ("offset", off)]);
    let n = s.call("3d_sdf_op_rounding", &[("sdf", m), ("radius", r)]);
    at(&s.out(n), [1.75, 0.0, 0.0], 0.0, "rounded offset");

    // a function of the file wrapping a transform: its shape input is still evaluated where the transform wants it
    let mut w = Scene::new();
    let mut input = materialcraft::graph::Node { kind: "f_input".into(), ..Default::default() };
    input.strs.insert("name".into(), "shape".into());
    let (inp, off) = (w.add(input), w.vec3(0.0, 0.0, 5.0));
    let t = w.call("3d_sdf_transform_offset", &[("sdf", inp), ("offset", off)]);
    w.set_out(t);
    let mut s = Scene::new();
    s.lib.insert("move_up".into(), w.g);
    let (shape, col) = (s.sphere(0.5, [0.0; 3]), s.vec3(1.0, 0.0, 0.0));
    let shape = s.call("3d_sdf_set_color", &[("sdf_scene", shape), ("base_color", col)]);
    let mut n = materialcraft::graph::Node { kind: "f_call".into(), ..Default::default() };
    n.strs.insert("fn".into(), "move_up".into());
    n.ins.insert("a".into(), shape);
    let n = s.add(n);
    let wrapped = s.out(n);
    at(&wrapped, [0.0, 0.0, 5.0], -0.5, "wrapped transform");
    let (_, m) = wrapped.run_sdf(&FnEnv::new([256.0, 256.0]), [0.0, 0.0, 5.0]).unwrap();
    assert_eq!(m.color, [1.0, 0.0, 0.0]);
}

#[test]
fn sdf_repeat_and_symmetry_make_the_copies() {
    let mut s = Scene::new();
    let (shape, a, b) = (s.sphere(0.25, [0.5, 0.0, 0.0]), s.ivec3(2.0, 0.0, 0.0), s.ivec3(1.0, 0.0, 0.0));
    let n = s.call("3d_sdf_op_repeat_mirror", &[("sdf", shape), ("amount_plus", a), ("amount_minus", b)]);
    let r = s.out(n);
    at(&r, [0.5, 0.0, 0.0], -0.25, "the original");
    at(&r, [1.5, 0.0, 0.0], -0.25, "the first copy, mirrored across x = 1");
    at(&r, [4.5, 0.0, 0.0], -0.25, "the second copy");
    at(&r, [-2.5, 0.0, 0.0], -0.25, "the copy on the other side, mirrored across x = -1");
    at(&r, [-3.5, 0.0, 0.0], 0.75, "no second copy on the other side");
    at(&r, [8.5, 0.0, 0.0], 3.75, "no third copy");
    at(&r, [0.5, 2.0, 0.0], 1.75, "no copy on Y");

    let symmetry = |c: [f32; 3], axes: [f32; 3], pre: [f32; 3]| {
        let mut s = Scene::new();
        let (shape, m, o) = (s.sphere(0.25, c), s.ivec3(axes[0], axes[1], axes[2]), s.vec3(pre[0], pre[1], pre[2]));
        let n = s.call("3d_sdf_op_symmetry", &[("sdf", shape), ("mirror_axis", m), ("pre_offset", o)]);
        s.out(n)
    };
    let x = symmetry([1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0; 3]);
    at(&x, [1.0, 0.0, 0.0], -0.25, "symmetry: the shape");
    at(&x, [-1.0, 0.0, 0.0], -0.25, "symmetry: the duplicate");
    at(&x, [0.0, 0.0, 0.0], 0.75, "symmetry: between");
    // a shape across the plane is a union, not a fold: all of both is there
    let across = symmetry([0.1, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0; 3]);
    at(&across, [0.35, 0.0, 0.0], 0.0, "symmetry across the plane");
    at(&across, [-0.35, 0.0, 0.0], 0.0, "symmetry across the plane");
    // all three axes: eight shapes
    let all = symmetry([1.0, 2.0, 3.0], [1.0, 1.0, 1.0], [0.0; 3]);
    for (a, b, c) in [(1.0, 1.0, 1.0), (-1.0, 1.0, 1.0), (1.0, -1.0, 1.0), (-1.0, -1.0, 1.0), (1.0, 1.0, -1.0), (-1.0, 1.0, -1.0), (1.0, -1.0, -1.0), (-1.0, -1.0, -1.0)] {
        at(&all, [a, 2.0 * b, 3.0 * c], -0.25, "symmetry on three axes");
    }
    let pre = symmetry([0.0; 3], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]);
    at(&pre, [2.0, 0.0, 0.0], -0.25, "pre-offset");
    at(&pre, [-2.0, 0.0, 0.0], -0.25, "pre-offset");
}

#[test]
fn sdf_material_follows_the_shapes() {
    let env = FnEnv::new([256.0, 256.0]);
    // two spheres, red id 1 and blue id 2, on either side of the origin
    let two = |s: &mut Scene| {
        let (a, b, red, blue, one, two) = (s.sphere(0.5, [-2.0, 0.0, 0.0]), s.sphere(0.5, [2.0, 0.0, 0.0]), s.vec3(1.0, 0.0, 0.0), s.vec3(0.0, 0.0, 1.0), s.int(1.0), s.int(2.0));
        let a = s.call("3d_sdf_set_color", &[("sdf_scene", a), ("base_color", red)]);
        let a = s.call("3d_sdf_set_id", &[("sdf_scene", a), ("id", one)]);
        let b = s.call("3d_sdf_set_color", &[("sdf_scene", b), ("base_color", blue)]);
        let b = s.call("3d_sdf_set_id", &[("sdf_scene", b), ("id", two)]);
        (a, b)
    };
    let close = |a: [f64; 3], b: [f64; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-6);

    // nothing set: the default, and the distance is untouched
    let mut s = Scene::new();
    let n = s.sphere(0.5, [0.0; 3]);
    let (v, m) = s.out(n).run_sdf(&env, [0.0; 3]).unwrap();
    approx(&v, &[-0.5]);
    assert_eq!(m, materialcraft::func::SdfMaterial::default());

    // Set material sets the three, the distance goes through
    let mut s = Scene::new();
    let (n, c, mt, r) = (s.sphere(0.5, [0.0; 3]), s.vec3(0.1, 0.2, 0.3), s.float(0.75), s.float(0.25));
    let n = s.call("3d_sdf_set_material", &[("sdf_scene", n), ("base_color", c), ("metalness", mt), ("roughness", r)]);
    let (v, m) = s.out(n).run_sdf(&env, [0.5, 0.0, 0.0]).unwrap();
    approx(&v, &[0.0]);
    assert!(close(m.color, [0.1, 0.2, 0.3]), "{m:?}");
    assert_eq!((m.metalness, m.roughness), (0.75, 0.25));

    // one at a time, later nodes keep what earlier ones set
    let mut s = Scene::new();
    let (n, mt, r) = (s.sphere(0.5, [0.0; 3]), s.float(1.0), s.float(0.125));
    let n = s.call("3d_sdf_set_metalness", &[("sdf_scene", n), ("metalness", mt)]);
    let n = s.call("3d_sdf_set_roughness", &[("sdf_scene", n), ("roughness", r)]);
    let (_, m) = s.out(n).run_sdf(&env, [0.0; 3]).unwrap();
    assert_eq!((m.metalness, m.roughness), (1.0, 0.125));

    // a union is the material of the nearer shape
    let mut s = Scene::new();
    let (a, b) = two(&mut s);
    let n = s.call("3d_sdf_op_union", &[("sdf_1", a), ("sdf_2", b)]);
    let union = s.out(n);
    let (_, m) = union.run_sdf(&env, [-2.0, 0.0, 0.0]).unwrap();
    assert_eq!((m.color, m.id), ([1.0, 0.0, 0.0], 1));
    let (_, m) = union.run_sdf(&env, [2.0, 0.0, 0.0]).unwrap();
    assert_eq!((m.color, m.id), ([0.0, 0.0, 1.0], 2));

    // smooth: half way the colours blend, the id stays hard
    let mut s = Scene::new();
    let (a, b) = two(&mut s);
    let k = s.float(2.0);
    let n = s.call("3d_sdf_op_union_smooth", &[("sdf_1", a), ("sdf_2", b), ("smoothness", k)]);
    let smooth = s.out(n);
    let (_, m) = smooth.run_sdf(&env, [0.0, 0.0, 0.0]).unwrap();
    assert!(close(m.color, [0.5, 0.0, 0.5]), "{m:?}");
    let (_, m) = smooth.run_sdf(&env, [-0.5, 0.0, 0.0]).unwrap();
    assert!(close(m.color, [0.75, 0.0, 0.25]), "{m:?}");
    assert_eq!(m.id, 1);
    let (_, m) = smooth.run_sdf(&env, [0.5, 0.0, 0.0]).unwrap();
    assert_eq!(m.id, 2);

    // a subtraction keeps the material of what was cut
    let mut s = Scene::new();
    let (a, b) = two(&mut s);
    let n = s.call("3d_sdf_op_subtraction", &[("sdf_1", a), ("sdf_2", b)]);
    let (_, m) = s.out(n).run_sdf(&env, [2.0, 0.0, 0.0]).unwrap();
    assert_eq!(m.id, 1);

    // operators and transforms after the material keep it, in the space they moved it to
    let mut s = Scene::new();
    let (a, b) = two(&mut s);
    let (off, r, k) = (s.vec3(0.0, 5.0, 0.0), s.float(0.1), s.float(2.0));
    let u = s.call("3d_sdf_op_union", &[("sdf_1", a), ("sdf_2", b)]);
    let u = s.call("3d_sdf_op_rounding", &[("sdf", u), ("radius", r)]);
    let u = s.call("3d_sdf_transform_scale", &[("sdf", u), ("scale", k)]);
    let n = s.call("3d_sdf_transform_offset", &[("sdf", u), ("offset", off)]);
    let moved = s.out(n);
    let (_, m) = moved.run_sdf(&env, [4.0, 5.0, 0.0]).unwrap();
    assert_eq!(m.id, 2);
    let (_, m) = moved.run_sdf(&env, [-4.0, 5.0, 0.0]).unwrap();
    assert_eq!((m.color, m.id), ([1.0, 0.0, 0.0], 1));

    // a shape reached twice brings its material both times
    let mut s = Scene::new();
    let (shape, red) = (s.sphere(0.5, [0.0; 3]), s.vec3(1.0, 0.0, 0.0));
    let red = s.call("3d_sdf_set_color", &[("sdf_scene", shape), ("base_color", red)]);
    let n = s.call("3d_sdf_op_union", &[("sdf_1", red), ("sdf_2", red)]);
    let (_, m) = s.out(n).run_sdf(&env, [0.0; 3]).unwrap();
    assert_eq!(m.color, [1.0, 0.0, 0.0]);
}

#[test]
fn sdf_functions_are_all_offered_and_callable() {
    let pal = libfuncs::palette();
    for (cat, n) in [("SDF Primitives", 20), ("SDF Operators", 13), ("SDF Transforms", 9), ("SDF Material", 5)] {
        assert_eq!(pal.iter().filter(|e| e.category == cat).count(), n, "{cat}");
        assert!(libfuncs::CATEGORIES.contains(&cat));
    }
    let graphs = libfuncs::graphs();
    for e in pal.iter().filter(|e| e.category.starts_with("SDF")) {
        assert!(e.label.starts_with("SDF "), "{}", e.label);
        assert!(e.path.starts_with("Functions/SDF/"), "{}", e.path);
        let name = libfuncs::resolve(e.ident).unwrap();
        let prog = Prog::compile(&graphs[name], graphs).unwrap_or_else(|err| panic!("{}: {err}", e.ident));
        let ty = prog.result_type().unwrap_or_else(|err| panic!("{}: {err}", e.ident));
        assert_eq!(ty.lanes() as usize, if e.ident.ends_with("_p") { 3 } else { 1 }, "{}", e.ident);
        // on its defaults, nothing connected: a number
        let (v, _) = prog.run_sdf(&FnEnv::new([256.0, 256.0]), [0.25, 0.5, 0.75]).unwrap_or_else(|err| panic!("{}: {err}", e.ident));
        assert!(lanes_of(&v).iter().all(|x| x.is_finite()), "{}: {v:?}", e.ident);
    }
    // a function of a file with a common name is not taken for one of these
    assert!(libfuncs::resolve("sphere").is_none());
    assert!(libfuncs::resolve("union").is_none());
}

#[test]
fn sdf_functions_are_imported() {
    // Sphere > Offset > Union with a second sphere, P left to the host; inputs spelled as the reference names them
    std::env::remove_var("MC_SBS_LIBRARY");
    let dir = std::env::temp_dir().join(format!("mc_sdf_import_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let call = |uid: u32, ident: &str, conns: &[(&str, u32)]| {
        let links: String = conns.iter().map(|(k, v)| format!("<connection><identifier v=\"{k}\"/><connRef v=\"{v}\"/></connection>")).collect();
        format!("<paramNode><uid v=\"{uid}\"/><function v=\"instance\"/><connections>{links}</connections><funcDatas><funcData><name v=\"instance\"/><constantValue><constantValueString v=\"pkg:///Functions/SDF/{ident}?dependency=55\"/></constantValue></funcData></funcDatas></paramNode>")
    };
    let konst = |uid: u32, f: &str, tag: &str, v: &str| format!("<paramNode><uid v=\"{uid}\"/><function v=\"{f}\"/><funcDatas><funcData><name v=\"{f}\"/><constantValue><{tag} v=\"{v}\"/></constantValue></funcData></funcDatas></paramNode>");
    let nodes = [
        konst(1, "const_float1", "constantValueFloat1", "0.25"),
        konst(2, "const_float3", "constantValueFloat3", "0 0 2"),
        call(3, "Primitives/3d_sdf_sphere", &[("Radius", 1)]),
        call(4, "Transforms/3d_sdf_transform_offset", &[("sdf", 3), ("offset", 2)]),
        call(5, "Primitives/3d_sdf_sphere", &[]),
        call(6, "Operators/3d_sdf_op_union", &[("SDF 1", 4), ("sdf_2", 5)]),
    ]
    .concat();
    let main = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><package><identifier v=\"main\"/>\
        <dependencies><dependency><filename v=\"sbs://functions.sbs\"/><uid v=\"55\"/><type v=\"package\"/></dependency></dependencies><content>\
        <function><identifier v=\"scene\"/><uid v=\"9\"/><paramValue><dynamicValue><rootnode v=\"6\"/><paramNodes>{nodes}</paramNodes></dynamicValue></paramValue></function>\
        <graph><identifier v=\"g\"/><uid v=\"10\"/><compNodes/></graph></content></package>"
    );
    let path = dir.join("main.sbs");
    std::fs::write(&path, main).unwrap();
    let imported = materialcraft::sbs_import::import_file(&path).unwrap();
    let missing: Vec<_> = imported.report.notes.iter().filter(|n| n.text.contains("is not available")).map(|n| n.text.clone()).collect();
    assert!(missing.is_empty(), "{missing:?}");
    let g = imported.graph.subgraphs.get("scene").expect("function imported");
    let prog = Prog::compile(g, &imported.graph.subgraphs).unwrap();
    at(&prog, [0.0, 0.0, 2.0], -0.25, "imported, moved");
    at(&prog, [0.0, 0.0, 2.25], 0.0, "imported, moved");
    at(&prog, [0.0, 0.0, 0.0], -0.5, "imported, the default sphere");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn sdf_calls_added_in_the_editor_render_and_save() {
    use materialcraft::sbs_defs::PTy;
    use materialcraft::sbs_native::{self as native, PVal};
    let path = |ident: &str| libfuncs::palette().iter().find(|e| e.ident == ident).unwrap().path.clone();
    // a uniform colour driven by an Offset of a Sphere, added as the Library panel adds them, P left open
    let mut g = native::new_package("sdfcall", 16);
    g.nodes.insert(1, native::new_node("sbs:uniform", None, [0.0, 0.0]));
    let value = PVal::Const { ty: PTy::F(4), raw: "0 0 0 1".into(), rel: Some("0".into()) };
    let pkg = g.sd.as_ref().unwrap().pkg.clone();
    let name = native::make_function(&mut g.subgraphs, pkg, "sdfcall", 1, "outputcolor", &value);
    native::set_value(g.nodes.get_mut(&1).unwrap(), "outputcolor", Some(PVal::Dyn { graph: name.clone(), rel: Some("0".into()) }));
    {
        let fg = g.subgraphs.get_mut(&name).unwrap();
        fg.nodes.clear();
        let mut c = native::new_node("sbs:fn:const_float3", None, [0.0, 0.0]);
        native::set_value(&mut c, "const_float3", Some(PVal::Const { ty: PTy::F(3), raw: "0 0 2".into(), rel: None }));
        fg.nodes.insert(1, c);
        fg.nodes.insert(2, native::new_node("sbs:fn:instance", Some(&path("3d_sdf_sphere")), [100.0, 0.0]));
        let mut off = native::new_node("sbs:fn:instance", Some(&path("3d_sdf_transform_offset")), [200.0, 0.0]);
        assert_eq!(native::slots(&off, &BTreeMap::new()), ["sdf", "offset", "P"]);
        off.ins.insert("sdf".into(), 2);
        off.ins.insert("offset".into(), 1);
        fg.nodes.insert(3, off);
        fg.sd.as_mut().unwrap().root = Some(3);
        assert_eq!(native::label(&fg.nodes[&3], fg), "SDF Offset");
    }
    let check = |g: &materialcraft::graph::Graph, stage: &str| {
        let t = g.evaluate_node(1, 16).unwrap_or_else(|e| panic!("{stage}: {e:?}"));
        // no host: the sphere at (0, 0, 2), measured from the middle of the image on z = 0
        let want = (0.25f64 + 0.25 + 4.0).sqrt() - 0.5;
        assert!((t.px[0][0] as f64 - want).abs() < 1e-4, "{stage}: {} vs {want}", t.px[0][0]);
    };
    check(&g, "in the editor");
    let text = native::save_text(&g).unwrap();
    assert!(text.contains("sbs://functions.sbs"), "no dependency on the library package");
    assert!(text.contains("pkg:///Functions/SDF/Transforms/3d_sdf_transform_offset?dependency="), "the call does not name the library function");
    let (back, _) = native::load(&text, None).unwrap();
    check(&back, "after reopening");
    assert_eq!(native::save_text(&back).unwrap(), text, "saving again must not change the file");
}
