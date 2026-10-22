use materialcraft::graph::Graph;
fn stats(g: &str) -> (f32, f32) {
    let g = Graph::parse(g).unwrap();
    let o = g.evaluate(Some(256)).unwrap();
    let t = &o["o"];
    let v: Vec<f32> = t.px.iter().map(|p| p[0]).collect();
    (v.iter().sum::<f32>() / v.len() as f32, v.iter().cloned().fold(0.0, f32::max))
}
#[test]
fn edge_detect_shows_edges_on_soft_and_hard_inputs() {
    for (name, src) in [
        ("fbm", "size 256\nnode 1 fbm scale=4 octaves=4 seed=1\nnode 2 edge_detect in=@1\noutput o 2\n"),
        ("hard", "size 256\nnode 1 bricks rows=4 cols=4 mortar=0.05 bevel=0\nnode 2 edge_detect in=@1\noutput o 2\n"),
        ("shape", "size 256\nnode 1 shape\nnode 2 edge_detect in=@1\noutput o 2\n"),
    ] {
        let (mean, max) = stats(src);
        // every kind of input produces visible lines, and a flat picture produces none
        assert!(max > 0.5, "{name}: edge detect shows nothing");
        assert!(mean < 0.95 || name == "fbm", "{name}: everything is an edge");
    }
}

#[test]
fn edge_detect_tolerance_removes_weak_edges() {
    let at = |tol: f32| stats(&format!("size 256\nnode 1 fbm scale=4 octaves=4 seed=1\nnode 2 edge_detect in=@1 tolerance={tol}\noutput o 2\n")).0;
    assert!(at(0.0) > at(0.1) && at(0.1) > at(0.5), "{} {} {}", at(0.0), at(0.1), at(0.5));
    let flat = stats("size 64\nnode 1 color r=0.5 g=0.5 b=0.5\nnode 2 edge_detect in=@1\noutput o 2\n");
    assert_eq!(flat.1, 0.0);
}
