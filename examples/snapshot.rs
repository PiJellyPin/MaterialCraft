// SPDX-License-Identifier: MIT OR Apache-2.0
//! Regression snapshot: evaluates every node type's demo graph and every library node's preview and writes the raw
//! pixels, or compares them with a snapshot written before.
//! usage: snapshot write <dir> [size]   |   snapshot check <dir> [size]
use materialcraft::graph::{demo_graph_for, Graph, SPECS};
use std::path::Path;

fn pixels(g: &Graph, size: usize) -> Vec<(String, Vec<f32>)> {
    match g.evaluate(Some(size)) {
        Ok(m) => m.into_iter().map(|(k, t)| (k, t.px.iter().flat_map(|p| p.iter().copied()).collect())).collect(),
        Err(e) => vec![("error".into(), e.to_string().bytes().map(|b| b as f32).collect())],
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (mode, dir) = (a[1].as_str(), Path::new(&a[2]));
    let size: usize = a.get(3).and_then(|s| s.parse().ok()).unwrap_or(128);
    let mut graphs: Vec<(String, Graph)> = Vec::new();
    for sp in SPECS {
        if let Some(t) = demo_graph_for(sp.kind) {
            if let Ok(g) = Graph::parse(&t) {
                graphs.push((format!("node_{}", sp.kind), g));
            }
        }
    }
    std::fs::create_dir_all(dir).unwrap();
    let (mut same, mut differ, mut missing) = (0, 0, 0);
    for (name, g) in &graphs {
        for (out, px) in pixels(g, size) {
            let file = dir.join(format!("{name}__{out}.f32"));
            let bytes: Vec<u8> = px.iter().flat_map(|v| v.to_le_bytes()).collect();
            if mode == "write" {
                std::fs::write(&file, &bytes).unwrap();
                continue;
            }
            match std::fs::read(&file) {
                Ok(old) if old == bytes => same += 1,
                Ok(old) => {
                    differ += 1;
                    let o: Vec<f32> = old.chunks(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
                    let worst = if o.len() == px.len() { o.iter().zip(&px).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max) } else { f32::INFINITY };
                    let mean = if o.len() == px.len() { o.iter().zip(&px).map(|(a, b)| (a - b).abs() as f64).sum::<f64>() / px.len() as f64 } else { f64::INFINITY };
                    println!("DIFF {name} {out}: max {worst:.6} mean {mean:.7}");
                }
                Err(_) => missing += 1,
            }
        }
    }
    if mode != "write" {
        println!("identical {same}, different {differ}, missing {missing}");
    } else {
        println!("wrote snapshot of {} graphs", graphs.len());
    }
}
