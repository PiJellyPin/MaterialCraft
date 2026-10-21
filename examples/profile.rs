// SPDX-License-Identifier: MIT OR Apache-2.0
//! Editor-style profile of the built-in example graph and of the .sbs packages named on the command line:
//!   cold   full pass with an empty node cache (what opening a graph costs)
//!   warm   full pass again with nothing changed (pure overhead: hashing, cache lookups, thumbnails)
//!   edit   one parameter changed on a node in the middle of the graph, interactive pass (what a drag costs)
//!   settle the full pass that follows the edit
//! usage: profile [size] [filter] [package.sbs ...]
use materialcraft::graph::{Graph, NodeCache, EXAMPLE};
use std::sync::Arc;
use std::time::Instant;

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

/// Keeps the newest result alive (and drops the one before), as the editor does with what is on screen.
fn hold<T>(slot: &mut Option<T>, r: T) {
    *slot = Some(r);
}

fn main() {
    let size: usize = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(1024);
    let filter = std::env::args().nth(2).unwrap_or_default();
    let mut graphs: Vec<(String, Graph)> = vec![("example".into(), Graph::parse(EXAMPLE).unwrap())];
    for p in std::env::args().skip(3) {
        match materialcraft::sbs_native::open(std::path::Path::new(&p)) {
            Ok((g, _)) => graphs.push((p.clone(), g)),
            Err(e) => eprintln!("{p}: {e}"),
        }
    }
    let (mut tc, mut tw, mut te, mut ts) = (0.0, 0.0, 0.0, 0.0);
    println!("{:<24} {:>5} {:>9} {:>9} {:>9} {:>9}", "graph", "nodes", "cold ms", "warm ms", "edit ms", "settle ms");
    for (name, mut g) in graphs {
        if !filter.is_empty() && !name.contains(&filter) {
            continue;
        }
        let keep: Vec<u32> = g.output_nodes().iter().map(|o| o.2).collect();
        let cache = Arc::new(NodeCache::new(4096));
        let first = keep.first().copied();
        // like the editor, the previous result stays alive while the next pass runs
        let t = Instant::now();
        let mut held = None;
        hold(&mut held, g.evaluate_all_cached(size, &keep, 0, &cache, first, true, &mut |_| {}));
        let cold = ms(t);
        let t = Instant::now();
        hold(&mut held, g.evaluate_all_cached(size, &keep, 0, &cache, first, true, &mut |_| {}));
        let warm = ms(t);
        // a node roughly in the middle of the graph that has a numeric parameter
        let ids: Vec<u32> = g.nodes.iter().filter(|(_, n)| !n.nums.is_empty() && n.kind != "output").map(|(i, _)| *i).collect();
        let (edit, settle) = if let Some(&id) = ids.get(ids.len() / 2) {
            let n = g.nodes.get_mut(&id).unwrap();
            let k = n.nums.keys().next().unwrap().clone();
            let v = n.nums[&k];
            n.nums.insert(k, v + 0.013);
            let t = Instant::now();
            hold(&mut held, g.evaluate_all_cached(size, &keep, 0, &cache, first, false, &mut |_| {}));
            let e = ms(t);
            let t = Instant::now();
            hold(&mut held, g.evaluate_all_cached(size, &keep, 0, &cache, first, true, &mut |_| {}));
            (e, ms(t))
        } else {
            (0.0, 0.0)
        };
        drop(held);
        println!("{:<24} {:>5} {:>9.0} {:>9.0} {:>9.0} {:>9.0}", name, g.nodes.len(), cold, warm, edit, settle);
        tc += cold;
        tw += warm;
        te += edit;
        ts += settle;
    }
    println!("{:<24} {:>5} {:>9.0} {:>9.0} {:>9.0} {:>9.0}", "TOTAL", "", tc, tw, te, ts);
}
