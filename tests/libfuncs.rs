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
