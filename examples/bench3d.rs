// Timing of the edit -> 3D maps path without the GUI: graph evaluation (cold / after a one-node edit), then the
// conversion the viewer needs. Usage: bench3d <template> <size> [node-id-to-edit]
use materialcraft::gpu3d::{self, MapInputs};
use materialcraft::graph::NodeCache;
use materialcraft::sbs_import;
use std::sync::Arc;
use std::time::Instant;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let name = a.get(1).map(|s| s.as_str()).unwrap_or("cobblestone");
    let size: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(2048);
    let path = format!("assets/templates/{name}.sbs");
    let imp = sbs_import::import_file(std::path::Path::new(&path)).expect("import");
    let mut g = imp.graph;
    let outs = g.output_nodes();
    let keep: Vec<u32> = outs.iter().map(|o| o.2).collect();
    println!("{name}: {} nodes, outputs {:?}", g.nodes.len(), outs.iter().map(|o| o.0.clone()).collect::<Vec<_>>());
    let cache = Arc::new(NodeCache::with_default_budget());
    let t = Instant::now();
    let r = { materialcraft::tex::CURRENT_JOB.store(1, std::sync::atomic::Ordering::Relaxed); g.evaluate_all_cached(size, &keep, 1, &cache, None, false, &mut |_| {}) };
    println!("cold eval  {size}: {:>8.0} ms  errors {:?} kept {} sizes {}", t.elapsed().as_secs_f64() * 1e3, r.errors, r.kept.len(), r.sizes.len());
    // edit the first non-output node that has a numeric parameter, in the middle of the graph
    let target = a.get(3).and_then(|s| s.parse::<u32>().ok()).or_else(|| g.nodes.iter().filter(|(id, n)| !keep.contains(id) && !n.nums.is_empty()).map(|(id, _)| *id).next());
    if let Some(id) = target {
        let n = g.nodes.get_mut(&id).unwrap();
        let (k, v) = n.nums.iter().next().map(|(k, v)| (k.clone(), *v)).unwrap();
        n.nums.insert(k.clone(), v * 1.07 + 0.01);
        println!("edited node {id} ({}) param {k}", g.nodes[&id].kind);
    }
    let t = Instant::now();
    let r = { materialcraft::tex::CURRENT_JOB.store(2, std::sync::atomic::Ordering::Relaxed); g.evaluate_all_cached(size, &keep, 2, &cache, None, false, &mut |_| {}) };
    println!("edit eval  {size}: {:>8.0} ms", t.elapsed().as_secs_f64() * 1e3);
    let named = |n: &str| outs.iter().find(|o| o.0 == n).and_then(|o| r.kept.get(&o.2)).map(|a| a.as_ref());
    let t = Instant::now();
    let nrm = named("normal").and_then(|t| g.normal_for_view(t));
    println!("normal_for_view: {:>6.0} ms", t.elapsed().as_secs_f64() * 1e3);
    let t = Instant::now();
    let p = gpu3d::prepare(&MapInputs {
        base: named("basecolor"),
        normal: nrm.as_ref().or_else(|| named("normal")),
        rough: named("roughness"),
        metal: named("metallic"),
        ao: named("ambient_occlusion"),
        emissive: named("emissive"),
        height: named("height"),
    });
    println!("gpu prepare ({}): {:>6.0} ms", p.size, t.elapsed().as_secs_f64() * 1e3);
}
