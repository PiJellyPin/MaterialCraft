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
