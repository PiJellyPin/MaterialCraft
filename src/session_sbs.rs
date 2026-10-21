// SPDX-License-Identifier: MIT OR Apache-2.0
// ======================================================================= commands on .sbs packages (included into session.rs)
//
// A session over an `.sbs` package edits the file's own nodes and values (see `sbs_native`): node kinds are
// `sbs:<filter>`, `sbs:lib:<package>/<graph>`, `sbs:graph`, `sbs:input`, `sbs:output`, `sbs:fn:<function>`;
// parameters are the format's, with its value types.

use crate::sbs_defs::{self as sdefs, Fam as SFam, PTy};
use crate::sbs_native::{self as native, PVal};

fn sd_fam(g: &Graph) -> SFam {
    match g.graph_kind() {
        "function" => SFam::Function,
        "fxmap" => SFam::Pattern,
        _ => SFam::Texture,
    }
}

/// A value as JSON: numbers, a list of numbers, text, or `{"function": <graph>}`.
fn pval_json(v: &PVal) -> Json {
    match v {
        PVal::Dyn { graph, .. } => Json::obj().with("function", graph.as_str()),
        PVal::Const { ty: PTy::S | PTy::Path, raw, .. } => Json::from(raw.as_str()),
        PVal::Const { ty: PTy::B, .. } => Json::from(v.nums().first().copied().unwrap_or(0.0) != 0.0),
        PVal::Const { .. } => {
            let n = v.nums();
            if n.len() == 1 {
                Json::from(n[0])
            } else {
                Json::Arr(n.into_iter().map(Json::from).collect())
            }
        }
    }
}

/// A JSON value for parameter `p`: `None` restores the default.
fn json_pval(p: &native::ParamInfo, v: &Json) -> Result<Option<PVal>, String> {
    let lanes = match p.ty {
        PTy::FV => 4,
        t => t.lanes(),
    };
    let fill = |x: f32| -> Vec<f32> {
        match (p.ty, lanes) {
            (PTy::FV, _) => vec![x, x, x, 1.0],
            (_, 0) | (_, 1) => vec![x],
            (_, n) => vec![x; n],
        }
    };
    Ok(Some(match v {
        Json::Null => return Ok(None),
        Json::Bool(b) => native::value_from_nums(p, &[*b as i32 as f32]),
        Json::Num(_) => {
            let x = v.as_f32().ok_or_else(|| format!("{}: not a finite number", p.id))?;
            native::value_from_nums(p, &fill(x))
        }
        Json::Arr(a) => {
            let nums: Vec<f32> = a.iter().map(|x| x.as_f32().ok_or_else(|| format!("{}: a list of numbers", p.id))).collect::<Result<_, _>>()?;
            if lanes > 0 && nums.len() != lanes && !(p.ty == PTy::FV && (nums.len() == 1 || nums.len() == 3)) {
                return Err(format!("{}: {} numbers expected", p.id, lanes));
            }
            let nums = if p.ty == PTy::FV && nums.len() == 3 { vec![nums[0], nums[1], nums[2], 1.0] } else { nums };
            native::value_from_nums(p, &nums)
        }
        Json::Str(t) => {
            if !p.choices.is_empty() {
                let lower = t.trim().to_lowercase();
                let unused = crate::sbs_lib::UNUSED_CHOICE;
                let idx = p.choices.iter().position(|c| c != unused && c.to_lowercase() == lower).or_else(|| {
                    let m: Vec<usize> = p.choices.iter().enumerate().filter(|(_, c)| c.as_str() != unused && c.to_lowercase().starts_with(&lower)).map(|(i, _)| i).collect();
                    (m.len() == 1).then(|| m[0])
                });
                match (idx, lower.parse::<f32>()) {
                    (Some(i), _) => native::value_from_nums(p, &[i as f32]),
                    (None, Ok(x)) if x.is_finite() => native::value_from_nums(p, &[x]),
                    (None, _) => {
                        let names: Vec<&str> = p.choices.iter().map(String::as_str).filter(|c| *c != unused).collect();
                        return Err(format!("{}: '{t}' is not one of: {}", p.id, names.join(", ")));
                    }
                }
            } else if matches!(p.ty, PTy::S | PTy::Path) {
                PVal::Const { ty: PTy::S, raw: t.clone(), rel: Some("0".into()) }
            } else {
                let nums: Vec<f32> = t.split_whitespace().map(|x| x.parse::<f32>().map_err(|_| format!("{}: a number, not text", p.id))).collect::<Result<_, _>>()?;
                native::value_from_nums(p, &nums)
            }
        }
        Json::Obj(_) => return Err(format!("{}: to compute a value with a function use node.drive", p.id)),
    }))
}

/// The unit of a value when the name alone does not say it: angles of the format are turns (1 = 360 degrees).
fn param_unit(p: &native::ParamInfo) -> Option<&'static str> {
    let id = p.id.to_ascii_lowercase();
    if !p.choices.is_empty() || !matches!(p.ty, PTy::F(_)) {
        return None;
    }
    let small = p.range.map(|(a, b)| a >= -1.0001 && b <= 1.0001).unwrap_or(true);
    if (id.contains("rotation") || id.contains("angle") || id == "hue") && small && !id.contains("random") && !id.contains("multiplier") {
        return Some("turns (0.25 = 90 degrees, 1 = a full turn)");
    }
    if (id.contains("rotation") || id.contains("angle")) && id.contains("random") && small {
        return Some("turns of random spread (1 = any angle)");
    }
    None
}

fn param_info_json(p: &native::ParamInfo) -> Json {
    let mut j = Json::obj().with("name", p.id.as_str()).with("label", p.label.as_str()).with("type", p.ty.code()).with("default", p.def.as_str());
    if let Some((a, b)) = p.range {
        j.set("min", a);
        j.set("max", b);
    }
    if !p.choices.is_empty() {
        let unused = crate::sbs_lib::UNUSED_CHOICE;
        let sparse = p.choices.iter().any(|c| c == unused);
        let clean = |c: &str| c.trim_end_matches(" (assumed)").to_string();
        if sparse {
            // only the numbers that mean something; their numbers are in `choice_numbers`
            j.set("choices", Json::Arr(p.choices.iter().filter(|c| c.as_str() != unused).map(|c| Json::from(clean(c))).collect()));
            let used: Vec<Json> = p.choices.iter().enumerate().filter(|(_, c)| c.as_str() != unused).map(|(i, c)| Json::from(format!("{i}: {}", clean(c)))).collect();
            j.set("choice_numbers", Json::Arr(used));
            let assumed: Vec<Json> = p.choices.iter().filter(|c| c.ends_with(" (assumed)")).map(|c| Json::from(clean(c))).collect();
            if !assumed.is_empty() {
                j.set("not_yet_checked", Json::Arr(assumed));
            }
        } else {
            j.set("choices", Json::Arr(p.choices.iter().map(|c| Json::from(c.as_str())).collect()));
        }
        if let Some(cur) = p.current().nums().first() {
            if let Some(l) = p.choices.get(cur.round().max(0.0) as usize) {
                j.set("choice", l.trim_end_matches(" (assumed)"));
            }
        }
    }
    if let Some(u) = param_unit(p) {
        j.set("unit", u);
    }
    if !p.group.is_empty() {
        j.set("group", p.group.as_str());
    }
    if let Some(v) = &p.value {
        j.set("value", pval_json(v));
    }
    j
}

fn def_json(d: &sdefs::NodeDef) -> Json {
    let mut j = Json::obj().with("kind", d.kind).with("label", d.label).with("category", d.category).with("family", d.fam.name());
    j.set("inputs", Json::Arr(d.inputs.iter().map(|i| Json::from(if i.many { format!("{} (repeatable: {0}, {0}:1, {0}:2 ...)", i.id) } else { i.id.to_string() })).collect()));
    if d.outputs.len() > 1 {
        j.set("outputs", Json::Arr(d.outputs.iter().map(|o| Json::from(*o)).collect()));
    }
    if let Some((p, g)) = d.library {
        j.set("library", format!("{p}/{g}"));
    }
    if !d.renders {
        j.set("renders", false);
    }
    j
}

fn sd_node_types(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let q = p.get("query").and_then(Json::as_str).unwrap_or("").to_lowercase();
    let cat = p.get("category").and_then(Json::as_str).unwrap_or("").to_lowercase();
    let fam = match p.get("family").and_then(Json::as_str).map(|f| f.trim().to_lowercase()) {
        Some(f) if f.starts_with("func") => SFam::Function,
        Some(f) if f.starts_with("pat") || f.starts_with("fx") => SFam::Pattern,
        Some(_) => SFam::Texture,
        None if p.get("functions").and_then(Json::as_bool).unwrap_or(false) => SFam::Function,
        None => match s.focus.as_ref().and_then(|f| s.graph.subgraphs.get(f)) {
            Some(g) => sd_fam(g),
            None => sd_fam(&s.graph),
        },
    };
    let words: Vec<&str> = q.split_whitespace().collect();
    let mut cats: BTreeSet<&str> = BTreeSet::new();
    let mut list = Vec::new();
    let mut defs = sdefs::palette(fam);
    if fam == SFam::Texture {
        defs.extend(["sbs:input", "sbs:output", "sbs:graph"].iter().filter_map(|k| sdefs::def(k)));
    }
    for d in defs {
        cats.insert(d.category);
        if !cat.is_empty() && d.category.to_lowercase() != cat {
            continue;
        }
        let hay = format!("{} {} {} {}", d.kind, d.label, d.category, d.sd).to_lowercase().replace('_', " ");
        if !words.iter().all(|w| hay.contains(&w.replace('_', " "))) {
            continue;
        }
        list.push(def_json(d));
    }
    let note = match fam {
        SFam::Texture => "sbs:input / sbs:output add graph inputs and outputs (output.add is easier); sbs:graph runs another graph of the package (params {\"graph\": <name>})",
        SFam::Function => "function graphs give the value of their result node (graph.inspect shows it; node.set_result changes it)",
        SFam::Pattern => "FX-map graphs live inside FX-Map nodes",
    };
    Outcome::data(Json::obj().with("family", fam.name()).with("count", list.len()).with("categories", Json::Arr(cats.into_iter().map(Json::from).collect())).with("nodes", Json::Arr(list)).with("note", note))
}

fn sd_node_describe(kind: &str) -> Result<Outcome, String> {
    let d = sdefs::def(kind).ok_or_else(|| {
        let names: Vec<String> = sdefs::defs().list.iter().map(|d| d.kind.to_string()).collect();
        let hint = closest(kind, &names);
        if hint.is_empty() {
            format!("unknown node type '{kind}' (node.types lists them)")
        } else {
            format!("unknown node type '{kind}'. Did you mean: {}?", hint.join(", "))
        }
    })?;
    let mut j = def_json(d);
    let n = Node { kind: kind.to_string(), ..Default::default() };
    let mut params: Vec<Json> = native::params_of(&n, &BTreeMap::new()).iter().map(param_info_json).collect();
    if kind == "sbs:gradient" {
        params.insert(
            0,
            Json::obj()
                .with("name", "gradient_keys")
                .with("label", "Gradient colour keys")
                .with("type", "list")
                .with("default", "black at 0, white at 1")
                .with("format", "[{\"position\": 0..1, \"color\": [r, g, b, a] or a grey level}, ...] (or [[position, r, g, b, a], ...]); 1 to 64 keys, set with node.set"),
        );
    }
    j.set("params", Json::Arr(params));
    j.set("ranges", "min / max are the usual slider range: node.set accepts values outside it (it warns) and they are drawn as given; choices can be set by number or by name");
    let mut notes: Vec<&str> = Vec::new();
    match kind {
        "sbs:distance" => notes.push("distance is in pixels of a 256-pixel image (it scales with the output size, not with your pattern's cells): the result is 1 on the white shapes and falls to 0 that far outside them. To grow or shrink a mask by N pixels use macro.add grow / shrink."),
        "sbs:transformation" => notes.push("matrix22 maps the output back onto the input: [2, 0, 0, 2] shows the input twice per side (smaller), [0.5, 0, 0, 0.5] enlarges it. Only whole-number zooms keep a tiling texture seamless (macro.add tile_scale)."),
        "sbs:directionalwarp" | "sbs:warp" => notes.push("intensity has no upper limit; values above the slider range warp further. Strength counts pixels of a 256-pixel image, so the same value moves a texture the same share of its width at any size. Start near 1 to 5 and look at the result (render.node) before raising it; for a strong swirl use a smooth, low-frequency intensity map."),
        "sbs:blur" | "sbs:hqblur" | "sbs:slopeblur" => notes.push("intensity counts pixels of a 256-pixel image (the default 10 is a soft blur): it scales with the output size. Imported graphs are converted at about two thirds of that value (the format's blur is a box-like spread, ours a Gaussian); tune it by eye with render.node."),
        "sbs:pixelprocessor" | "sbs:valueprocessor" => notes.push("write its function as text with node.expression (param perpixel / function)"),
        "sbs:gradient" => notes.push("set the colour stops with gradient_keys (node.set)"),
        _ => {}
    }
    let no_scale = d.library.is_some()
        && d.inputs.is_empty()
        && d.category == "Generators"
        && !d.params.iter().any(|p| {
            let id = p.id.to_ascii_lowercase();
            id.contains("scale") || id.contains("tiling") || id.contains("amount") || id.contains("size")
        });
    if no_scale {
        notes.push("this noise has no scale setting: make its features smaller with macro.add tile_scale (a whole-number zoom keeps it tiling)");
    }
    if !notes.is_empty() {
        j.set("notes", Json::Arr(notes.into_iter().map(Json::from).collect::<Vec<_>>()));
    }
    Outcome::data(j)
}

fn sd_describe_node(s: &Session, id: u32, n: &Node, compact: bool) -> Json {
    let mut j = Json::obj().with("id", id);
    if let Some(a) = s.alias_of(id) {
        j.set("name", a);
    }
    j.set("kind", n.kind.as_str());
    j.set("label", native::label(n, &s.graph));
    if let Some(t) = n.strs.get("title") {
        j.set("title", t.as_str());
    }
    if let Some(g) = n.strs.get("#graph") {
        j.set("graph", g.as_str());
    }
    let mut pr = Json::obj();
    for (k, v) in native::values(n) {
        pr.set(&k, pval_json(&v));
    }
    if n.kind == "sbs:gradient" {
        pr.set("gradient_keys", gradient_keys_json(n));
    }
    j.set("params", pr);
    let mut ins = Json::obj();
    for (slot, src) in &n.ins {
        ins.set(
            slot,
            match n.in_out.get(slot) {
                Some(o) => Json::obj().with("node", *src).with("output", o.as_str()),
                None => Json::from(*src),
            },
        );
    }
    j.set("inputs", ins);
    if !compact {
        j.set("x", n.pos[0]);
        j.set("y", n.pos[1]);
    }
    j
}

fn sd_problems(g: &Graph) -> Vec<String> {
    let mut v = Vec::new();
    let fam = sd_fam(g);
    for (id, n) in &g.nodes {
        if sdefs::def(&n.kind).is_none() {
            v.push(format!("node {id}: unknown type '{}' (kept as it is in the file)", n.kind));
            continue;
        }
        if sdefs::family(&n.kind) != Some(fam) {
            v.push(format!("node {id} ({}): does not belong in a {} graph", n.kind, fam.name()));
        }
        let slots = native::slots(n, &g.subgraphs);
        for (slot, src) in &n.ins {
            if !slots.contains(slot) {
                v.push(format!("node {id} ({}): has no input '{slot}' (inputs: {})", n.kind, slots.join(", ")));
            }
            if !g.nodes.contains_key(src) {
                v.push(format!("node {id}: input '{slot}' comes from node {src}, which does not exist"));
            }
        }
        if n.kind == "sbs:graph" {
            if let Some(t) = n.strs.get("#graph") {
                if !g.subgraphs.contains_key(t) {
                    v.push(format!("node {id}: runs graph '{t}', which the package does not have"));
                }
            }
        }
        for (k, val) in native::values(n) {
            if let PVal::Dyn { graph, .. } = val {
                if !g.subgraphs.contains_key(&graph) {
                    v.push(format!("node {id}: '{k}' is computed by graph '{graph}', which is missing"));
                }
            }
        }
    }
    if let Err(e) = g.check_cycles() {
        v.push(e.to_string());
    }
    match fam {
        SFam::Texture => {
            if g.output_nodes().is_empty() && g.sd.as_ref().map(|s| s.role == native::Role::Graph).unwrap_or(false) {
                v.push("the graph has no outputs (output.add makes a node an output)".into());
            }
        }
        _ => {
            if g.sd.as_ref().and_then(|s| s.root).map(|r| !g.nodes.contains_key(&r)).unwrap_or(true) && !g.nodes.is_empty() {
                v.push("no node gives the graph's result (node.set_result)".into());
            }
        }
    }
    v
}

/// Things that are allowed but probably not meant: nodes whose result reaches no output (or, in a function graph,
/// not the result node).
fn sd_warnings(g: &Graph) -> Vec<String> {
    let mut v = Vec::new();
    let fam = sd_fam(g);
    let roots: Vec<u32> = match fam {
        SFam::Texture => g.nodes.iter().filter(|(_, n)| n.kind == "sbs:output").map(|(i, _)| *i).collect(),
        _ => g.sd.as_ref().and_then(|s| s.root).into_iter().collect(),
    };
    let mut used: BTreeSet<u32> = BTreeSet::new();
    let mut stack = roots.clone();
    while let Some(i) = stack.pop() {
        if !used.insert(i) {
            continue;
        }
        if let Some(n) = g.nodes.get(&i) {
            stack.extend(n.ins.values().copied());
        }
    }
    if !roots.is_empty() {
        for (id, n) in &g.nodes {
            if used.contains(id) || n.kind == "sbs:output" {
                continue;
            }
            let what = match fam {
                SFam::Texture => "reaches no output",
                _ => "does not reach the result node",
            };
            v.push(format!("node {id} ({}) {what}: its result is not used (connect it or remove it)", native::label(n, g)));
        }
    }
    v
}

fn sd_graph_inspect(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let compact = p.get("compact").and_then(Json::as_bool).unwrap_or(false);
    let g = &s.graph;
    let nodes: Vec<Json> = g.nodes.iter().map(|(id, n)| sd_describe_node(s, *id, n, compact)).collect();
    let outs: Vec<Json> = g.output_nodes().into_iter().map(|(usage, ident, id)| Json::obj().with("usage", usage).with("name", ident).with("node", id)).collect();
    let mut j = Json::obj().with("graph", s.current_name()).with("format", "sbs").with("kind", sd_fam(g).name());
    j.set("size", g.size);
    if let Some(sd) = &g.sd {
        if let Some(r) = sd.root {
            j.set("result_node", r);
        }
        let mut base = Json::obj();
        for (k, v) in &sd.base {
            if let Some(pv) = PVal::dec(v) {
                base.set(k, pval_json(&pv));
            }
        }
        j.set("settings", base);
    }
    let ins: Vec<Json> = native::value_inputs(g).into_iter().map(|(id, label, ty, def)| Json::obj().with("identifier", id).with("label", label).with("type", ty.code()).with("default", def)).collect();
    if !ins.is_empty() {
        j.set("parameters", Json::Arr(ins));
    }
    j.set("file", s.path.as_ref().map(|p| p.display().to_string()));
    j.set("unsaved_changes", s.dirty);
    j.set("nodes", Json::Arr(nodes));
    j.set("outputs", Json::Arr(outs));
    if !g.comments.is_empty() {
        j.set("comments", Json::Arr(g.comments.iter().map(|c| Json::obj().with("text", c.text.as_str()).with("x", c.pos[0]).with("y", c.pos[1])).collect::<Vec<_>>()));
    }
    j.set("problems", Json::Arr(sd_problems(g).into_iter().map(Json::from).collect::<Vec<_>>()));
    Outcome::data(j)
}

/// Is `slot` an input of node `n`? Repeatable inputs take any number (`input:3`).
fn sd_check_slot(n: &Node, lib: &BTreeMap<String, Graph>, slot: &str) -> Result<(), String> {
    let slots = native::slots(n, lib);
    if slots.iter().any(|s| s == slot) {
        return Ok(());
    }
    if let (Some(d), Some((base, k))) = (sdefs::def(&n.kind), slot.split_once(':')) {
        if d.inputs.iter().any(|i| i.many && i.id == base) && k.parse::<u32>().map(|k| (1..64).contains(&k)).unwrap_or(false) {
            return Ok(());
        }
    }
    Err(if slots.is_empty() { format!("{} has no inputs", n.kind) } else { format!("{} has no input '{slot}' (inputs: {})", n.kind, slots.join(", ")) })
}

fn sd_set_link(g: &mut Graph, to: u32, slot: &str, from: u32, out: Option<String>) -> Result<(), String> {
    if from == to {
        return Err("a node cannot feed itself".into());
    }
    let tn = g.nodes.get(&to).cloned().ok_or("no such target node")?;
    sd_check_slot(&tn, &g.subgraphs, slot)?;
    let sn = g.nodes.get(&from).cloned().ok_or("no such source node")?;
    if sn.kind == "sbs:output" {
        return Err("an output node has no output to link from".into());
    }
    if let Some(o) = &out {
        let outs = native::outputs(&sn, &g.subgraphs);
        if !outs.contains(o) {
            return Err(format!("node {from} has no output '{o}' (outputs: {})", outs.join(", ")));
        }
    }
    let n = g.nodes.get_mut(&to).ok_or("no such target node")?;
    let old = (n.ins.get(slot).copied(), n.in_out.get(slot).cloned());
    n.ins.insert(slot.to_string(), from);
    match out {
        Some(o) => {
            n.in_out.insert(slot.to_string(), o);
        }
        None => {
            n.in_out.remove(slot);
        }
    }
    if let Err(e) = g.check_cycles() {
        if let Some(n) = g.nodes.get_mut(&to) {
            match old.0 {
                Some(o) => {
                    n.ins.insert(slot.to_string(), o);
                }
                None => {
                    n.ins.remove(slot);
                }
            }
            match old.1 {
                Some(o) => {
                    n.in_out.insert(slot.to_string(), o);
                }
                None => {
                    n.in_out.remove(slot);
                }
            }
        }
        return Err(format!("that connection would make a loop ({e})"));
    }
    Ok(())
}

/// Apply JSON values to a native node. Returns what was clamped or ignored.
/// The keys of a Gradient Map node as `[{position, color}]`.
fn gradient_keys_json(n: &Node) -> Json {
    Json::Arr(native::gradient_keys(n).into_iter().map(|(p, c)| Json::obj().with("position", p).with("color", Json::Arr(c.iter().map(|x| Json::from(*x)).collect::<Vec<_>>()))).collect::<Vec<_>>())
}

/// Gradient keys from `[{position, color}]` or `[[position, r, g, b, a]]` (a grey level is one number).
fn json_gradient_keys(v: &Json) -> Result<Vec<(f32, [f32; 4])>, String> {
    let bad = || "gradient_keys: a list of {\"position\": 0..1, \"color\": [r, g, b, a] or a grey level}".to_string();
    let arr = v.as_arr().ok_or_else(bad)?;
    if arr.is_empty() || arr.len() > 64 {
        return Err("gradient_keys: 1 to 64 keys".into());
    }
    let color = |c: &Json| -> Result<[f32; 4], String> {
        let nums: Vec<f32> = match c {
            Json::Arr(a) => a.iter().map(|x| x.as_f32().filter(|f| f.is_finite()).ok_or_else(bad)).collect::<Result<_, _>>()?,
            x => vec![x.as_f32().filter(|f| f.is_finite()).ok_or_else(bad)?],
        };
        Ok(match nums.len() {
            1 => [nums[0], nums[0], nums[0], 1.0],
            3 => [nums[0], nums[1], nums[2], 1.0],
            4 => [nums[0], nums[1], nums[2], nums[3]],
            _ => return Err(bad()),
        })
    };
    arr.iter()
        .map(|k| match k {
            Json::Arr(a) if a.len() >= 2 => {
                let pos = a[0].as_f32().filter(|f| f.is_finite()).ok_or_else(bad)?;
                Ok((pos, color(&Json::Arr(a[1..].to_vec()))?))
            }
            Json::Obj(_) => {
                let pos = k.get("position").and_then(Json::as_f32).filter(|f| f.is_finite()).ok_or_else(bad)?;
                Ok((pos, color(k.get("color").ok_or_else(bad)?)?))
            }
            _ => Err(bad()),
        })
        .collect()
}

fn sd_apply(n: &mut Node, lib: &BTreeMap<String, Graph>, values: &[(String, Json)], allow_unknown: bool) -> Result<Vec<String>, String> {
    let infos = native::params_of(n, lib);
    let mut warn = Vec::new();
    for (k, v) in values {
        let info = match infos.iter().find(|p| p.id == *k).cloned() {
            Some(i) => i,
            None if k == "gradient_keys" && n.kind == "sbs:gradient" => {
                native::set_gradient_keys(n, &json_gradient_keys(v)?);
                continue;
            }
            None if k == "title" => {
                match v.as_str() {
                    Some(t) if !t.is_empty() => n.strs.insert("title".into(), t.to_string()),
                    _ => n.strs.remove("title"),
                };
                continue;
            }
            None if allow_unknown => native::ParamInfo { id: k.clone(), label: k.clone(), ty: if v.as_str().is_some() { PTy::S } else { PTy::F(1) }, def: String::new(), range: None, choices: Vec::new(), group: String::new(), value: None, rel: false },
            None => {
                let names: Vec<String> = infos.iter().map(|p| p.id.clone()).collect();
                let hint = closest(k, &names);
                return Err(format!("{} has no parameter '{k}'.{} (node.describe / node.params list them)", n.kind, if hint.is_empty() { String::new() } else { format!(" Did you mean: {}?", hint.join(", ")) }));
            }
        };
        if matches!(info.value, Some(PVal::Dyn { .. })) && !v.is_null() {
            warn.push(format!("{k} was computed by a function; it is a constant now"));
        }
        let val = json_pval(&info, v)?;
        if let (Some(PVal::Const { .. }), Some((lo, hi))) = (&val, info.range) {
            if let Some(x) = val.as_ref().and_then(|v| v.nums().first().copied()) {
                if x < lo || x > hi {
                    warn.push(format!("{k}: {x} is outside the usual range {lo}..{hi}"));
                }
            }
        }
        native::set_value(n, k, val);
    }
    Ok(warn)
}

fn sd_node_add(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let kind = p.get("kind").and_then(Json::as_str).unwrap_or("").to_string();
    let kind = if kind.starts_with("sbs:") { kind } else { format!("sbs:{kind}") };
    let d = sdefs::def(&kind).ok_or_else(|| format!("unknown node type '{kind}' (node.types lists them; .sbs packages use the format's node types)"))?;
    let kind = d.kind.to_string();
    let (nf, gf) = (d.fam, sd_fam(&s.graph));
    if nf != gf {
        return Err(format!("{kind} is a {} node and graph '{}' is a {} graph", nf.name(), s.current_name(), gf.name()));
    }
    let name = p.get("name").and_then(Json::as_str).map(str::to_string);
    if let Some(n) = &name {
        if n.is_empty() || n.parse::<u32>().is_ok() || n.starts_with('@') || n.len() > 64 {
            return Err("a node name must be text that is not a number (at most 64 characters)".into());
        }
        if let Some(old) = s.aliases.get(n) {
            if s.graph.nodes.contains_key(old) {
                return Err(format!("the name '{n}' is already used by node {old}"));
            }
        }
    }
    let params: Vec<(String, Json)> = p.get("params").and_then(Json::as_obj).map(|o| o.to_vec()).unwrap_or_default();
    let get = |k: &str| params.iter().find(|(x, _)| x == k).map(|(_, v)| v.clone());
    let mut links: Vec<(String, u32, Option<String>)> = Vec::new();
    if let Some(Json::Obj(o)) = p.get("inputs") {
        for (slot, v) in o {
            let (src, out) = parse_link(s, v).map_err(|e| format!("inputs.{slot}: {e}"))?;
            links.push((slot.clone(), src, out));
        }
    }
    let map: BTreeMap<String, u32> = links.iter().map(|(k, v, _)| (k.clone(), *v)).collect();
    let pos = match (p.get("x").and_then(Json::as_f32), p.get("y").and_then(Json::as_f32)) {
        (Some(x), Some(y)) => [x, y],
        _ => free_pos(&s.graph, &map),
    };
    let rest: Vec<(String, Json)> = params.iter().filter(|(k, _)| !matches!(k.as_str(), "usage" | "identifier" | "gray" | "graph")).cloned().collect();
    let id = match kind.as_str() {
        "sbs:output" => {
            let usage = get("usage").and_then(|v| v.as_str().map(str::to_string)).unwrap_or_else(|| "baseColor".into());
            let ident = get("identifier").and_then(|v| v.as_str().map(str::to_string));
            native::add_output(&mut s.graph, &usage, ident.as_deref(), pos)
        }
        "sbs:input" => {
            let gray = get("gray").and_then(|v| v.as_bool()).unwrap_or(false);
            let ident = get("identifier").and_then(|v| v.as_str().map(str::to_string));
            native::add_input(&mut s.graph, gray, ident.as_deref(), pos)
        }
        "sbs:graph" => {
            let g = get("graph").and_then(|v| v.as_str().map(str::to_string)).ok_or("a graph node needs params {\"graph\": <name of a graph of the package>}")?;
            match s.graph.subgraphs.get(&g) {
                Some(t) if t.sd.as_ref().map(|x| x.role == native::Role::Graph).unwrap_or(false) => {}
                _ => return Err(format!("the package has no graph '{g}' to run (graph.list)")),
            }
            let me = native::graph_name(&s.graph);
            if native::uses_graph(&native::all_graphs(&s.graph), &g, &me) {
                return Err(format!("graph '{g}' uses '{me}' itself, so it cannot run inside it"));
            }
            let id = native::free_id(&s.graph);
            s.graph.nodes.insert(id, native::new_node("sbs:graph", Some(&g), pos));
            id
        }
        k => {
            let id = native::free_id(&s.graph);
            s.graph.nodes.insert(id, native::new_node(k, None, pos));
            id
        }
    };
    let mut warnings = Vec::new();
    if !rest.is_empty() {
        let lib = s.graph.subgraphs.clone();
        let n = s.graph.nodes.get_mut(&id).ok_or("no such node")?;
        warnings = sd_apply(n, &lib, &rest, false)?;
    }
    // the graph's default format, unless the call set one
    if let Some(code) = s.graph.meta.get("mc_default_format").and_then(|c| c.parse::<i32>().ok()) {
        if !matches!(kind.as_str(), "sbs:output" | "sbs:input" | "sbs:graph" | "sbs:passthrough") && !kind.starts_with("sbs:fn:") && !kind.starts_with("sbs:fx:") && !rest.iter().any(|(k, _)| k == "format") {
            let lib = s.graph.subgraphs.clone();
            if let Some(n) = s.graph.nodes.get_mut(&id) {
                let _ = sd_apply(n, &lib, &[("format".to_string(), Json::from(code))], false);
            }
        }
    }
    for (slot, src, out) in links {
        let slot = if kind == "sbs:output" && slot == "in" { "inputNodeOutput".to_string() } else { slot };
        sd_set_link(&mut s.graph, id, &slot, src, out).map_err(|e| format!("inputs.{slot}: {e}"))?;
    }
    if let Some(n) = name {
        s.aliases.insert(n, id);
    }
    let mut j = Json::obj().with("id", id).with("kind", kind);
    // an FX-Map comes with its pattern graph (one Quadrant), edited with graph=<fx_graph>
    if let Some(fx) = native::ensure_fx_graph(&mut s.graph, id) {
        j.set("fx_graph", fx);
        j.set("next", "node.add / node.set / node.connect with graph=<fx_graph> (sbs:fx:paramset Quadrant, sbs:fx:addnode Iterate, sbs:fx:markov2 Switch), node.set_result for the node at the top");
    }
    if !warnings.is_empty() {
        j.set("warnings", Json::Arr(warnings.into_iter().map(Json::from).collect::<Vec<_>>()));
    }
    Outcome::data(j)
}

fn sd_node_set(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let id = s.arg_node(p, NODE)?;
    let values = p.get("params").and_then(Json::as_obj).ok_or("params must be an object")?.to_vec();
    if values.is_empty() {
        return Err("params is empty".into());
    }
    let allow = p.get("allow_unknown").and_then(Json::as_bool).unwrap_or(false);
    let lib = s.graph.subgraphs.clone();
    let n = s.graph.nodes.get_mut(&id).ok_or("no such node")?;
    let warnings = sd_apply(n, &lib, &values, allow)?;
    let mut j = Json::obj().with("id", id).with("updated", values.len());
    if !warnings.is_empty() {
        j.set("warnings", Json::Arr(warnings.into_iter().map(Json::from).collect::<Vec<_>>()));
    }
    Outcome::data(j)
}

fn sd_node_params(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let id = s.arg_node(p, NODE)?;
    let n = &s.graph.nodes[&id];
    let list: Vec<Json> = native::params_of(n, &s.graph.subgraphs).iter().map(param_info_json).collect();
    let mut j = Json::obj().with("node", id).with("kind", n.kind.as_str()).with("params", Json::Arr(list));
    if n.kind == "sbs:gradient" {
        j.set("gradient_keys", gradient_keys_json(n));
        j.set("gradient_keys_note", "set them with node.set params {\"gradient_keys\": [{\"position\": 0, \"color\": [r, g, b, a]}, ...]}");
    }
    j.set("inputs", Json::Arr(native::slots(n, &s.graph.subgraphs).into_iter().map(Json::from).collect::<Vec<_>>()));
    let outs = native::outputs(n, &s.graph.subgraphs);
    if outs.len() > 1 {
        j.set("outputs", Json::Arr(outs.into_iter().map(Json::from).collect::<Vec<_>>()));
    }
    Outcome::data(j)
}

/// Compute a value with a function graph of its own (made from the value it has now), or stop doing so.
fn sd_node_drive(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let id = s.arg_node(p, NODE)?;
    let param = p.get("param").and_then(Json::as_str).unwrap_or("").trim().to_string();
    let n = s.graph.nodes[&id].clone();
    let info = native::params_of(&n, &s.graph.subgraphs).into_iter().find(|x| x.id == param).ok_or_else(|| format!("{} has no parameter '{param}'", n.kind))?;
    if p.get("stop").and_then(Json::as_bool).unwrap_or(false) || p.get("function").and_then(Json::as_str) == Some("") {
        if !matches!(info.value, Some(PVal::Dyn { .. })) {
            return Err(format!("'{param}' is not computed by a function"));
        }
        if let Some(n) = s.graph.nodes.get_mut(&id) {
            native::set_value(n, &param, None);
        }
        return Outcome::data(Json::obj().with("node", id).with("param", param).with("driven", false).with("value", "default"));
    }
    if p.get("function").is_some() {
        return Err("in .sbs packages each value has its own function graph: leave out 'function' to make it, then add nodes to it with graph=<the returned name> (sbs:fn:* nodes; call a package function with sbs:fn:instance)".into());
    }
    if let Some(PVal::Dyn { graph, .. }) = &info.value {
        return Outcome::data(Json::obj().with("node", id).with("param", param).with("function_graph", graph.as_str()).with("existing", true));
    }
    let owner = s.graph.meta.get("identifier").cloned().unwrap_or_default();
    let pkg = s.graph.sd.as_ref().map(|x| x.pkg.clone()).ok_or("not an .sbs graph")?;
    let cur = info.current();
    let name = native::make_function(&mut s.graph.subgraphs, pkg, &owner, id, &param, &cur);
    if let Some(n) = s.graph.nodes.get_mut(&id) {
        native::set_value(n, &param, Some(PVal::Dyn { graph: name.clone(), rel: Some("0".into()) }));
    }
    Outcome::data(Json::obj().with("node", id).with("param", param).with("function_graph", name).with("next", "node.add / node.connect with graph=<function_graph> (sbs:fn:* nodes), then node.set_result for the node that gives the value; it starts as one constant of the current value"))
}

/// Turn a math expression into the function graph of a value (or rebuild a package function from one).
fn sd_node_expression(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let src = p.get("expression").and_then(Json::as_str).unwrap_or("").to_string();
    // the inputs of the graph the function belongs to can be read by identifier
    let mut inputs: BTreeMap<String, PTy> = native::value_inputs(&s.graph).into_iter().map(|(id, _, ty, _)| (id, if ty == PTy::FV { PTy::F(4) } else { ty })).collect();
    let (fname, want) = match p.get("function").and_then(Json::as_str) {
        Some(f) => {
            let g = s.graph.subgraphs.get(f).ok_or_else(|| format!("the package has no function '{f}' (graph.list)"))?;
            if g.graph_kind() != "function" {
                return Err(format!("'{f}' is not a function graph"));
            }
            inputs.extend(native::value_inputs(g).into_iter().map(|(id, _, ty, _)| (id, ty)));
            (f.to_string(), None)
        }
        None => {
            let id = s.arg_node(p, NODE)?;
            let param = p.get("param").and_then(Json::as_str).unwrap_or("").trim().to_string();
            if param.is_empty() {
                return Err("give node and param (the value to compute), or function (a function graph to rebuild)".into());
            }
            let n = s.graph.nodes[&id].clone();
            let info = native::params_of(&n, &s.graph.subgraphs).into_iter().find(|x| x.id == param).ok_or_else(|| format!("{} has no parameter '{param}'", n.kind))?;
            if matches!(info.ty, PTy::S | PTy::Path) {
                return Err(format!("'{param}' is text; expressions compute numbers"));
            }
            // a grayscale Pixel / Value Processor computes one float
            let grey = matches!(n.kind.as_str(), "sbs:pixelprocessor" | "sbs:valueprocessor") && native::values(&n).get("colorswitch").map(|v| v.nums().first().copied().unwrap_or(1.0) == 0.0).unwrap_or(false);
            let want = if info.ty == PTy::FV && grey { PTy::F(1) } else { info.ty };
            let fname = match &info.value {
                Some(PVal::Dyn { graph, .. }) => graph.clone(),
                _ => {
                    let owner = s.graph.meta.get("identifier").cloned().unwrap_or_default();
                    let pkg = s.graph.sd.as_ref().map(|x| x.pkg.clone()).ok_or("not an .sbs graph")?;
                    let cur = info.current();
                    let name = native::make_function(&mut s.graph.subgraphs, pkg, &owner, id, &param, &cur);
                    if let Some(n) = s.graph.nodes.get_mut(&id) {
                        native::set_value(n, &param, Some(PVal::Dyn { graph: name.clone(), rel: Some("0".into()) }));
                    }
                    name
                }
            };
            (fname, Some(want))
        }
    };
    let fg = s.graph.subgraphs.get_mut(&fname).ok_or("the function graph is missing")?;
    let lookup = |n: &str| inputs.get(n).copied();
    let (count, ty) = crate::fexpr::build(fg, &src, want, &lookup)?;
    let outline = crate::fexpr::outline(fg);
    Outcome::data(Json::obj().with("function_graph", fname).with("nodes", count).with("type", ty.code()).with("outline", outline).with("note", "edit it further with node.* commands and graph=<function_graph>, or call node.expression again"))
}

/// An identifier made from a label: lower case letters, digits and _.
fn ident_of(label: &str, fallback: &str) -> String {
    let mut t: String = label.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    while t.contains("__") {
        t = t.replace("__", "_");
    }
    let t = t.trim_matches('_').to_string();
    if t.is_empty() || t.starts_with(|c: char| c.is_ascii_digit()) {
        fallback.to_string()
    } else {
        t
    }
}

/// Move nodes of the graph into a new graph of the package and (by default) put a node running it in their place:
/// links from outside become the new graph's inputs, links to outside (and to graph outputs) its outputs.
fn sd_graph_extract(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    if sd_fam(&s.graph) != SFam::Texture {
        return Err("graph.extract works on texture graphs (function graphs are rebuilt with node.expression or graph.create kind=function)".into());
    }
    let ids: BTreeSet<u32> = s.nodes_arg(p, "nodes")?.into_iter().collect();
    if ids.is_empty() {
        return Err("nodes: give the nodes to move".into());
    }
    for id in &ids {
        let k = s.graph.nodes[id].kind.as_str();
        if matches!(k, "sbs:output" | "sbs:input") {
            return Err(format!("node {id} is a graph input or output; leave it out (the new graph gets its own)"));
        }
    }
    let name = p.get("name").and_then(Json::as_str).unwrap_or("").trim().to_string();
    let replace = p.get("replace").and_then(Json::as_bool).unwrap_or(true);
    // the links that cross the border, grouped by where they come from
    type Key = (u32, Option<String>);
    let mut ins: Vec<(Key, Vec<(u32, String)>)> = Vec::new();
    let mut outs: Vec<(Key, Vec<(u32, String)>)> = Vec::new();
    for (id, n) in &s.graph.nodes {
        for (slot, src) in &n.ins {
            let key: Key = (*src, n.in_out.get(slot).cloned());
            let list = if ids.contains(id) && !ids.contains(src) {
                &mut ins
            } else if !ids.contains(id) && ids.contains(src) {
                &mut outs
            } else {
                continue;
            };
            match list.iter_mut().find(|(k, _)| *k == key) {
                Some((_, c)) => c.push((*id, slot.clone())),
                None => list.push((key, vec![(*id, slot.clone())])),
            }
        }
    }
    let gray = |s: &Session, k: &Key| s.eval(k.0, 16, k.1.as_deref()).map(|t| t.px.iter().all(|q| (q[0] - q[1]).abs() < 1e-4 && (q[1] - q[2]).abs() < 1e-4)).unwrap_or(false);
    let in_gray: Vec<bool> = ins.iter().map(|(k, _)| gray(s, k)).collect();
    let out_gray: Vec<bool> = outs.iter().map(|(k, _)| gray(s, k)).collect();
    native::create_graph(&mut s.graph, &name, "graph")?;
    let mut ng = s.graph.subgraphs.remove(&name).ok_or("the new graph is missing")?;
    // copies of the nodes (with their own function graphs), links between them kept
    let map: BTreeMap<u32, u32> = ids.iter().enumerate().map(|(k, id)| (*id, k as u32 + 1)).collect();
    for id in &ids {
        let src = s.graph.nodes[id].clone();
        let nid = map[id];
        let mut c = native::copy_node(&src, &name, nid, &mut s.graph.subgraphs);
        c.ins.clear();
        c.in_out.clear();
        for (slot, from) in &src.ins {
            if let Some(m) = map.get(from) {
                c.ins.insert(slot.clone(), *m);
                if let Some(o) = src.in_out.get(slot) {
                    c.in_out.insert(slot.clone(), o.clone());
                }
            }
        }
        ng.nodes.insert(nid, c);
    }
    let (minx, maxx) = ids.iter().map(|i| s.graph.nodes[i].pos[0]).fold((f32::MAX, f32::MIN), |a, x| (a.0.min(x), a.1.max(x)));
    let cy = ids.iter().map(|i| s.graph.nodes[i].pos[1]).sum::<f32>() / ids.len() as f32;
    // graph inputs for what comes in, graph outputs for what goes out
    let mut in_idents = Vec::new();
    for (k, ((src, _), consumers)) in ins.iter().enumerate() {
        let slot_name = consumers.first().map(|c| c.1.split(':').next().unwrap_or("input").to_string()).unwrap_or_default();
        let label = s.alias_of(*src).map(str::to_string).unwrap_or_else(|| native::label(&s.graph.nodes[src], &s.graph));
        let ident = ident_of(&label, &ident_of(&slot_name, "input"));
        let iid = native::add_input(&mut ng, in_gray[k], Some(&ident), [minx - 200.0, cy + k as f32 * 140.0]);
        let actual = ng.sd.as_ref().and_then(|sd| sd.inputs.last()).and_then(|e| e.v("identifier")).unwrap_or(ident);
        for (cid, slot) in consumers {
            if let Some(n) = ng.nodes.get_mut(&map[cid]) {
                n.ins.insert(slot.clone(), iid);
                n.in_out.remove(slot);
            }
        }
        in_idents.push(actual);
    }
    let mut out_idents = Vec::new();
    for (k, ((src, out), consumers)) in outs.iter().enumerate() {
        // an output that fed a graph output keeps its usage
        let usage = consumers.iter().find_map(|(cid, _)| s.graph.output_nodes().into_iter().find(|o| o.2 == *cid).map(|o| o.0));
        let label = s.alias_of(*src).map(str::to_string).unwrap_or_else(|| native::label(&s.graph.nodes[src], &s.graph));
        let ident = ident_of(usage.as_deref().unwrap_or(&label), "output");
        let oid = native::add_output_ex(&mut ng, usage.as_deref().unwrap_or(&ident), Some(&ident), Some(out_gray[k]), [maxx + 200.0, cy + k as f32 * 140.0]);
        let actual = ng.sd.as_ref().and_then(|sd| sd.outputs.last()).and_then(|e| e.v("identifier")).unwrap_or(ident);
        if let Some(n) = ng.nodes.get_mut(&oid) {
            n.ins.insert("inputNodeOutput".into(), map[src]);
            if let Some(o) = out {
                n.in_out.insert("inputNodeOutput".into(), o.clone());
            }
        }
        out_idents.push(actual);
    }
    s.graph.subgraphs.insert(name.clone(), ng);
    // the node names move along
    let moved: BTreeMap<String, u32> = s.aliases.iter().filter_map(|(a, i)| map.get(i).map(|n| (a.clone(), *n))).collect();
    let mut j = Json::obj().with("created", name.as_str()).with("moved_nodes", ids.len());
    j.set("inputs", Json::Arr(in_idents.iter().map(|x| Json::from(x.as_str())).collect::<Vec<_>>()));
    j.set("outputs", Json::Arr(out_idents.iter().map(|x| Json::from(x.as_str())).collect::<Vec<_>>()));
    j.set("node_ids_in_new_graph", Json::Arr(ids.iter().map(|i| Json::obj().with("was", *i).with("now", map[i])).collect::<Vec<_>>()));
    if replace {
        let pos = [(minx + maxx) * 0.5, cy];
        native::remove_nodes(&mut s.graph, &ids);
        s.aliases.retain(|_, v| !ids.contains(v));
        let gid = native::free_id(&s.graph);
        let mut gn = native::new_node("sbs:graph", Some(&name), pos);
        for (k, ((src, out), _)) in ins.iter().enumerate() {
            gn.ins.insert(in_idents[k].clone(), *src);
            if let Some(o) = out {
                gn.in_out.insert(in_idents[k].clone(), o.clone());
            }
        }
        s.graph.nodes.insert(gid, gn);
        for (k, (_, consumers)) in outs.iter().enumerate() {
            for (cid, slot) in consumers {
                if let Some(n) = s.graph.nodes.get_mut(cid) {
                    n.ins.insert(slot.clone(), gid);
                    n.in_out.insert(slot.clone(), out_idents[k].clone());
                }
            }
        }
        if let Some(a) = p.get("node_name").and_then(Json::as_str).filter(|a| !a.is_empty() && a.parse::<u32>().is_err()) {
            s.aliases.insert(a.to_string(), gid);
        }
        j.set("node", gid);
    }
    if !moved.is_empty() {
        s.other_aliases.entry(name.clone()).or_default().extend(moved);
    }
    j.set("next", format!("edit it with graph={name} (or graph.focus); the node running it takes its inputs and outputs by those names"));
    Outcome::data(j)
}

/// The graph as text: one line per node (its values that differ from the defaults, with the names of numbered
/// choices, and where each input comes from), then the outputs.
fn sd_outline(s: &Session) -> String {
    let g = &s.graph;
    let mut out = String::new();
    let name_of = |id: u32| s.alias_of(id).map(|a| format!(" \"{a}\"")).unwrap_or_default();
    for (id, n) in &g.nodes {
        if n.kind == "sbs:output" {
            continue;
        }
        let infos = native::params_of(n, &g.subgraphs);
        let mut vals = Vec::new();
        for (k, v) in native::values(n) {
            let shown = match &v {
                PVal::Dyn { graph, .. } => format!("<function {graph}>"),
                PVal::Const { raw, .. } => {
                    let label = infos.iter().find(|i| i.id == k).and_then(|i| {
                        let c = v.nums().first().copied()?;
                        i.choices.get(c.round().max(0.0) as usize).cloned()
                    });
                    match label {
                        Some(l) => format!("{raw} ({l})"),
                        None => crate::review::clip(raw, 48),
                    }
                }
            };
            vals.push(format!("{k}={shown}"));
        }
        if n.kind == "sbs:gradient" {
            vals.push(format!("gradient_keys={}", native::gradient_keys(n).len()));
        }
        let ins: Vec<String> = n
            .ins
            .iter()
            .map(|(slot, src)| match n.in_out.get(slot) {
                Some(o) => format!("{slot}<-#{src}.{o}"),
                None => format!("{slot}<-#{src}"),
            })
            .collect();
        out.push_str(&format!("#{id}{} {} [{}]", name_of(*id), native::label(n, g), n.kind));
        if !vals.is_empty() {
            out.push_str(&format!(" {}", vals.join(" ")));
        }
        if !ins.is_empty() {
            out.push_str(&format!(" | in: {}", ins.join(", ")));
        }
        out.push('\n');
    }
    let outs: Vec<String> = g
        .output_nodes()
        .into_iter()
        .map(|(usage, ident, id)| {
            let n = g.nodes.get(&id);
            let src = n.and_then(|n| n.ins.values().next().copied());
            let out = n.and_then(|n| n.in_out.values().next().cloned()).map(|o| format!(".{o}")).unwrap_or_default();
            match src {
                Some(sid) => format!("{usage} ({ident}) <- #{sid}{out}"),
                None => format!("{usage} ({ident}) <- nothing"),
            }
        })
        .collect();
    out.push_str(&format!("outputs: {}\n", if outs.is_empty() { "none".to_string() } else { outs.join(", ") }));
    out
}

/// Draw the node graph: a box per node with its thumbnail and label, curves for the links, outputs marked.
fn draw_graph(s: &Session, thumbs: &std::collections::HashMap<u32, std::sync::Arc<Tex>>, width: usize) -> Tex {
    use crate::review::{Canvas, ACCENT, DIM, PANEL, TEXT};
    let g = &s.graph;
    let (bw, bh, th) = (112.0f32, 96.0f32, 60usize);
    if g.nodes.is_empty() {
        let mut c = Canvas::new(width, 60, PANEL);
        c.text(12, 20, "(the graph has no nodes)", 2, DIM);
        return c.into_tex();
    }
    // a layered layout of its own, so the picture is readable whatever positions the nodes have (nodes added by
    // commands have none): a node sits one column right of its furthest input; columns that do not fit the width
    // continue in a band below
    let margin = 16.0f32;
    let (gx, gy) = (30.0f32, 14.0f32);
    let mut depth: std::collections::BTreeMap<u32, usize> = g.nodes.keys().map(|k| (*k, 0)).collect();
    for _ in 0..g.nodes.len().min(400) {
        let mut moved = false;
        for (id, n) in &g.nodes {
            let d = n.ins.values().filter_map(|s| depth.get(s)).map(|d| d + 1).max().unwrap_or(0);
            if d > depth[id] && d <= g.nodes.len() {
                depth.insert(*id, d);
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
    let ncols = depth.values().copied().max().unwrap_or(0) + 1;
    let cap = (((width as f32 - 2.0 * margin) / (bw + gx)).floor() as usize).max(1);
    let mut row_of: std::collections::BTreeMap<u32, f32> = std::collections::BTreeMap::new();
    let mut cols: Vec<Vec<u32>> = vec![Vec::new(); ncols];
    for d in 0..ncols {
        let mut ids: Vec<(f32, u32)> = depth.iter().filter(|(_, dd)| **dd == d).map(|(id, _)| {
            let srcs: Vec<f32> = g.nodes[id].ins.values().filter_map(|s| row_of.get(s).copied()).collect();
            let score = if srcs.is_empty() { *id as f32 * 0.001 } else { srcs.iter().sum::<f32>() / srcs.len() as f32 + *id as f32 * 1e-4 };
            (score, *id)
        }).collect();
        ids.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        for (i, (_, id)) in ids.iter().enumerate() {
            row_of.insert(*id, i as f32);
            cols[d].push(*id);
        }
    }
    let bands = ncols.div_ceil(cap);
    let mut band_y = vec![0.0f32; bands + 1];
    for b in 0..bands {
        let rows = (b * cap..((b + 1) * cap).min(ncols)).map(|d| cols[d].len()).max().unwrap_or(1).max(1);
        band_y[b + 1] = band_y[b] + rows as f32 * (bh + gy) + 10.0;
    }
    let mut place: std::collections::BTreeMap<u32, (f32, f32)> = std::collections::BTreeMap::new();
    for (d, ids) in cols.iter().enumerate() {
        let (band, col) = (d / cap, d % cap);
        for (r, id) in ids.iter().enumerate() {
            place.insert(*id, (margin + col as f32 * (bw + gx), margin + 26.0 + band_y[band] + r as f32 * (bh + gy)));
        }
    }
    let height = (band_y[bands] + 2.0 * margin + 26.0).clamp(140.0, 6000.0) as usize;
    let mut c = Canvas::new(width, height, PANEL);
    c.text(12, 6, "node graph", 2, TEXT);
    // links first, under the boxes
    for (id, n) in &g.nodes {
        let (tx, ty) = place[id];
        for (k, src) in n.ins.values().enumerate() {
            if g.nodes.contains_key(src) {
                let (sx, sy) = place[src];
                let slot_y = ty + 20.0 + (k as f32 * 12.0).min(bh - 28.0);
                c.link(sx + bw, sy + bh * 0.5, tx, slot_y, [0.55, 0.58, 0.62, 0.9]);
            }
        }
    }
    for (id, n) in &g.nodes {
        let (x, y) = place[id];
        let (xi, yi) = (x as i64, y as i64);
        let out = n.kind == "sbs:output";
        c.rect(xi, yi, bw as i64, bh as i64, [0.21, 0.22, 0.24, 1.0]);
        c.frame(xi, yi, bw as i64, bh as i64, if out { ACCENT } else { [0.36, 0.38, 0.42, 1.0] }, 2);
        let label = if out {
            g.output_nodes().into_iter().find(|o| o.2 == *id).map(|o| format!("out: {}", o.0)).unwrap_or_else(|| "output".into())
        } else {
            match s.alias_of(*id) {
                Some(a) => a.to_string(),
                None => native::label(n, g),
            }
        };
        c.text(xi + 4, yi + 4, &crate::review::clip(&format!("#{id} {label}"), 17), 1, TEXT);
        if let Some(t) = thumbs.get(id) {
            c.blit(t, xi + (bw as i64 - th as i64) / 2, yi + 18, th, th);
        } else if !out {
            c.text(xi + 8, yi + 44, "(no picture)", 1, DIM);
        }
    }
    c.into_tex()
}

/// The review sheet of the graph (see `render.review`).
fn sd_render_review(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    use crate::review::{self as rv, Canvas, ACCENT, BG, DIM, TEXT};
    if sd_fam(&s.graph) != SFam::Texture {
        return Err("render.review shows texture graphs (with outputs); for a function or FX-map graph use render.node".into());
    }
    let size = opt_usize(p, "size", 128, 1024, 384)?;
    let tex = opt_usize(p, "tex", 32, 2048, 512)?;
    let thumbs_on = p.get("thumbnails").and_then(Json::as_bool).unwrap_or(true);
    let view = view_from(&view_params(&s.graph, p))?;
    s.graph.validate().map_err(|e| format!("cannot render yet: {e}"))?;
    let mm = crate::cli3d::MaterialMaps::of(&s.graph, tex)?;
    let render = crate::cli3d::render_picture(&mm, &view, size, size, false, 64);
    // the reference picture, if any
    let reference = match p.get("reference").and_then(Json::as_str).filter(|r| !r.is_empty()) {
        Some(r) => {
            let path = s.resolve_path(r)?;
            Some(crate::imgio::load(&path).map_err(|e| format!("reference: {}: {e}", path.display()))?)
        }
        None => None,
    };
    let compare_mode = p.get("compare").and_then(Json::as_str).unwrap_or("auto").to_string();
    if !matches!(compare_mode.as_str(), "auto" | "render" | "basecolor") {
        return Err("compare: auto, render or basecolor".into());
    }
    // thumbnails of every node, computed together
    let mut thumbs = std::collections::HashMap::new();
    let mut notes: Vec<String> = Vec::new();
    if let Ok(low) = native::lower(&s.graph) {
        let here = s.current_name();
        for n in &low.report.notes {
            if (n.graph.is_empty() || n.graph == here) && n.level != crate::sbs_import::Level::Info && !n.text.contains("uses MaterialCraft's own") {
                let t = match n.node {
                    Some(id) => format!("node {id}: {}", n.text),
                    None => n.text.clone(),
                };
                if !notes.contains(&t) && notes.len() < 40 {
                    notes.push(t);
                }
            }
        }
        if thumbs_on && s.graph.nodes.len() <= 200 {
            let ids: Vec<u32> = s.graph.nodes.keys().copied().filter(|i| low.graph.nodes.contains_key(i)).collect();
            let r = low.graph.evaluate_all_cached(128, &ids, 0, &s.cache, None, false, &mut |_| {});
            thumbs = r.kept;
        }
    }
    // the maps, in the usual order then the rest
    let order = ["basecolor", "normal", "roughness", "metallic", "height", "ambient_occlusion", "emissive", "opacity"];
    let mut names: Vec<String> = order.iter().filter(|n| mm.output(n).is_some()).map(|n| n.to_string()).collect();
    for n in mm.names() {
        if !names.contains(&n) {
            names.push(n);
        }
    }
    let mut maps_json = Json::obj();
    for n in &names {
        if let Some(t) = mm.output(n) {
            let mut j = stats_json(t);
            j.set("look", rv::look(t, false).json());
            maps_json.set(n, j);
        }
    }
    // comparison with the reference
    let mut cmp_json = None;
    let mut hints: Vec<String> = Vec::new();
    let mut score_text = String::new();
    let match_lighting = p.get("match_lighting").and_then(Json::as_bool).unwrap_or(true);
    let mut crop_note: Option<[f32; 4]> = None;
    if let Some(r) = &reference {
        // measure the subject only: the box the caller gave, else what stands out from the plain background
        let rect = match p.get("crop") {
            Some(Json::Obj(_)) => {
                let c = p.get("crop").cloned().unwrap_or_else(Json::obj);
                let g = |k: &str, d: f32| c.get(k).and_then(Json::as_f32).unwrap_or(d);
                let (x, y) = (g("x", 0.0), g("y", 0.0));
                Some([x, y, g("w", 1.0 - x), g("h", 1.0 - y)])
            }
            Some(_) => return Err("crop: an object {x, y, w, h} with fractions of the reference picture".into()),
            None => rv::object_box(r),
        };
        crop_note = rect;
        let measured = match rect {
            Some(b) => rv::crop_frac(r, b),
            None => r.clone(),
        };
        let rl = rv::look(&measured, true);
        let lum_of = |m: [f32; 3]| 0.2126 * m[0] + 0.7152 * m[1] + 0.0722 * m[2];
        let mut candidates: Vec<(&str, rv::Look)> = Vec::new();
        let mut exposure_hint: Option<String> = None;
        if compare_mode != "basecolor" {
            let mut mid = rv::centre_crop(&render, 0.5);
            let ours_l = rv::look(&mid, true);
            let (bo, br) = (lum_of(ours_l.mean), lum_of(rl.mean));
            if match_lighting && bo > 1e-3 && br > 1e-3 {
                // the render's brightness depends on the preview lights: compare colour, contrast and detail after
                // matching it, and say what exposure would do it
                let gain = (br / bo).clamp(0.2, 5.0);
                if (gain - 1.0).abs() > 0.08 {
                    exposure_hint = Some(format!("lighting: the preview is {} than the reference (brightness {:.2} vs {:.2}); exposure={:.2} (or light_intensity) would match it, so colour, contrast and detail below were compared after matching. Judge the material's own brightness by the base colour map", if gain > 1.0 { "darker" } else { "brighter" }, bo, br, gain));
                    mid = rv::exposure_gain(&mid, gain);
                }
            }
            candidates.push(("render", rv::look(&mid, true)));
        }
        if compare_mode != "render" {
            if let Some(b) = mm.output("basecolor") {
                candidates.push(("basecolor", rv::look(b, false)));
            }
        }
        let mut best: Option<(f32, &str, Json, Vec<String>)> = None;
        let mut all = Json::obj();
        for (what, l) in &candidates {
            let (score, j, h) = rv::compare(l, &rl);
            all.set(what, j.clone());
            if best.as_ref().map(|b| score > b.0).unwrap_or(true) {
                best = Some((score, what, j, h));
            }
        }
        if let Some((score, what, _, mut h)) = best {
            if what == "render" {
                if let Some(e) = exposure_hint {
                    h.insert(0, e);
                }
            }
            if let Some(b) = rect {
                h.push(format!("the reference was measured on its subject only (x {:.2} y {:.2} w {:.2} h {:.2} of the picture; pass crop to change it)", b[0], b[1], b[2], b[3]));
            }
            hints = h;
            score_text = format!("similarity {:.2} ({what} vs reference)", score);
            let mut cj = all;
            cj.set("best_match", what);
            cj.set("similarity", (score * 1000.0).round() / 1000.0);
            cj.set("hints", Json::Arr(hints.iter().map(|x| Json::from(x.as_str())).collect::<Vec<_>>()));
            if let Some(b) = rect {
                cj.set("reference_crop", Json::Arr(b.iter().map(|v| Json::from(*v)).collect::<Vec<_>>()));
            }
            cmp_json = Some(cj);
        }
    }
    // ---------------------------------------------------------------- the picture
    let pad = 12i64;
    let info_w = 420i64;
    let top_w = pad + size as i64 + pad + if reference.is_some() { size as i64 + pad } else { 0 } + info_w + pad;
    let cell = 150i64;
    let maps_w = pad + names.len().max(1) as i64 * (cell + pad);
    let width = top_w.max(maps_w).max(900) as usize;
    let graph_pic = draw_graph(s, &thumbs, width - 2 * pad as usize);
    let top_h = 30 + size as i64 + pad;
    let maps_h = 30 + cell + 18 + pad;
    let height = (pad + top_h + maps_h + graph_pic.h as i64 + pad) as usize;
    let mut c = Canvas::new(width, height, BG);
    let title = format!("{}  -  review", s.current_name());
    c.text(pad, pad, &rv::clip(&title, 70), 2, TEXT);
    let y = pad + 30;
    c.blit(&render, pad, y, size, size);
    c.text(pad + 4, y + 4, "3D preview", 1, DIM);
    let mut x = pad + size as i64 + pad;
    if let Some(r) = &reference {
        // keep the reference's proportions inside a square
        let (rw, rh) = if r.w >= r.h { (size, (size * r.h / r.w.max(1)).max(1)) } else { ((size * r.w / r.h.max(1)).max(1), size) };
        c.rect(x, y, size as i64, size as i64, [0.08, 0.08, 0.09, 1.0]);
        let (ox, oy) = (x + (size as i64 - rw as i64) / 2, y + (size as i64 - rh as i64) / 2);
        c.blit(r, ox, oy, rw, rh);
        if let Some(b) = crop_note {
            // the part that was measured
            c.frame(ox + (b[0] * rw as f32) as i64, oy + (b[1] * rh as f32) as i64, (b[2] * rw as f32) as i64, (b[3] * rh as f32) as i64, ACCENT, 1);
        }
        c.text(x + 4, y + 4, "reference", 1, ACCENT);
        x += size as i64 + pad;
    }
    // the info panel: score, hints, problems
    let mut ty = y;
    let mut lines: Vec<(String, [f32; 4])> = Vec::new();
    if !score_text.is_empty() {
        lines.push((score_text.clone(), ACCENT));
        for h in &hints {
            lines.push((format!("- {h}"), TEXT));
        }
    }
    let problems = sd_problems(&s.graph);
    let warnings = sd_warnings(&s.graph);
    for pr in &problems {
        lines.push((format!("problem: {pr}"), [1.0, 0.45, 0.4, 1.0]));
    }
    for w in &warnings {
        lines.push((format!("warning: {w}"), [0.95, 0.8, 0.4, 1.0]));
    }
    if lines.is_empty() {
        lines.push(("no reference given: pass reference=<image> to compare".into(), DIM));
    }
    let chars = ((info_w - 8) / 6) as usize;
    for (text, col) in lines {
        // wrap at word boundaries
        let mut cur = String::new();
        for word in text.split_whitespace() {
            if cur.len() + word.len() + 1 > chars && !cur.is_empty() {
                if ty < y + size as i64 - 12 {
                    c.text(x, ty, &cur, 1, col);
                }
                ty += 13;
                cur = format!("  {word}");
            } else {
                if !cur.is_empty() {
                    cur.push(' ');
                }
                cur.push_str(word);
            }
        }
        if ty < y + size as i64 - 12 {
            c.text(x, ty, &cur, 1, col);
        }
        ty += 17;
    }
    // the maps
    let my = pad + top_h;
    c.text(pad, my, "output maps", 2, TEXT);
    for (i, n) in names.iter().enumerate() {
        let cx = pad + i as i64 * (cell + pad);
        if let Some(t) = mm.output(n) {
            c.blit(t, cx, my + 30, cell as usize, cell as usize);
        }
        c.text(cx, my + 30 + cell + 4, &rv::clip(n, 24), 1, TEXT);
    }
    // the graph
    let gy = my + maps_h;
    c.blit(&graph_pic, pad, gy, graph_pic.w, graph_pic.h);
    let pic = c.into_tex();
    // ---------------------------------------------------------------- the data
    let mut j = Json::obj().with("graph", s.current_name()).with("outline", sd_outline(s));
    j.set("render", rv::look(&rv::centre_crop(&render, 0.5), true).json());
    j.set("maps", maps_json);
    if let Some(cj) = cmp_json {
        j.set("comparison", cj);
    }
    j.set("problems", Json::Arr(problems.into_iter().map(Json::from).collect::<Vec<_>>()));
    j.set("warnings", Json::Arr(warnings.into_iter().map(Json::from).collect::<Vec<_>>()));
    if !notes.is_empty() {
        j.set("approximations", Json::Arr(notes.into_iter().map(Json::from).collect::<Vec<_>>()));
    }
    j.set("view", view.to_text());
    j.set("picture", format!("{}x{} PNG: 3D preview{}, output maps, node graph with a picture per node", pic.w, pic.h, if reference.is_some() { ", the reference" } else { "" }));
    Ok(Outcome { data: j, images: vec![Image { label: "review".into(), png: png_of(&pic) }] })
}

/// Ready-made groups of standard nodes (`macro.add`): name, options, what it builds.
pub const MACROS: &[(&str, &str, &str)] = &[
    ("grow", "input (mask node), pixels (default 4)", "dilate a mask: Distance then Levels at the half-way point grows the white shapes by `pixels` (pixels of a 256 px image, so it scales with the size)"),
    ("shrink", "input (mask node), pixels (default 4)", "erode a mask: Levels (invert), Distance, Levels (threshold), Levels (invert) shrinks the white shapes by `pixels`"),
    ("outline", "input (mask node), pixels (default 2)", "a line along the edge of the white shapes: grow minus shrink, with Blend (subtract)"),
    ("star", "points (default 5), inner (inner radius / outer, default 0.45), size (outer radius, default 0.45), rotation (turns)", "a star with any number of points: a grayscale Pixel Processor whose function is made from an expression"),
    ("polygon", "sides (default 6), scale (default 1), rotation (turns), bevel (0 flat .. 1, a bevelled edge, default 0)", "a flat polygon with any number of sides: the Polygon 2 library node"),
    ("tile_shape", "input (shape node), x_amount, y_amount (default 4), offset (row offset 0..1, default 0), size (default 1)", "repeat your own shape in a grid: Tile Generator with Pattern = Image input and the shape on Pattern Input 1 (use it for interlocking or custom tiles)"),
    ("strokes", "x_amount, y_amount (default 8), angle (turns, default 0), angle_random (default 0.1), length (default 0.9), width (default 0.2), position_random (default 0.3)", "directional brush strokes, one per cell: Tile Sampler with the Capsule pattern stretched along the stroke"),
    ("tile_scale", "input (node), times (whole number, default 2)", "make a pattern or noise that has no scale setting smaller by repeating it `times` per side: Transformation 2D with a whole-number zoom, so it still tiles"),
    ("bricks", "rows (default 8), columns (even number, default 4), bond (0 stack, 0.5 running, default 0.5), mortar (default 0.08 of a brick height), bevel (default 0.05), split (chance a brick is cut in halves, default 0), merge (chance two bricks join, default 0), height_random (default 0.2), chips (edge chipping 0..1, default 0.3), seed", "a brick or masonry height map in one Pixel Processor: courses, running or stacked bond, mortar, rounded bevel, half and double bricks, a random height per brick and chipped edges. Tiles."),
    ("cracks", "cells (default 6), width (default 0.02), reach (how much of the net shows, 0..1, default 0.5), roughness (default 0.3), seed", "cracks that taper to a point: the edges of a Voronoi net, thick where a noise is high and fading to nothing where it is low (white cracks on black, soft ends)"),
    ("plates", "cells (default 6), bevel (default 0.15), gap (default 0.03), height_random (default 0.5), dome (default 0.3), seed", "scattered plates: each Voronoi cell is a plate with its own height and a slight dome, a bevelled edge and a gap between plates (height map)"),
    ("fractured_rock", "cells (default 5), crack_width (default 0.03), erosion (default 0.3), height_random (default 0.4), seed", "fractured rock: plates with cracks cut between them and a noise eroding the surface (height map)"),
    ("moss", "density (cells per side, default 40), size (default 0.9), clumping (0 even .. 1 tight clumps, default 0.6), clump_scale (whole number, default 4), seed", "moss or foliage: small leaves scattered in a jittered grid and kept in clumps by a low-frequency noise (height map)"),
    ("strata", "layers (default 12), tilt (default 0.1), waviness (default 0.3), step (height difference between layers, default 0.5), edge (default 0.1), seed", "sedimentary layers: horizontal bands of random height, wavy and tilted, with a lit edge on each band"),
    ("edge_chip", "input (height or mask node), amount (default 0.5), scale (whole number, default 8), band (pixels, default 2), seed", "chipped edges: keeps the edges of the input where a noise is high. Returns the chip mask (`output_node`) and `band`, a thin line around each chip (a second node, listed in `outputs`)"),
    ("paint_chip", "input (optional height or mask node of where paint is thin), amount (default 0.5), scale (whole number, default 6), thickness (default 2), light_angle (turns, default 0.125), seed", "layered paint or plaster chipping: a chip mask plus a highlight band on the side facing the light and a shadow band on the other (nodes listed in `outputs`)"),
    ("flood_gray", "input (a Flood Fill node), min (default 0), max (default 1), contrast (0..1, default 0), seed", "a random grey per region of a Flood Fill with a limited luminance range and contrast: Flood Fill To Grayscale at full randomness, then Levels (input window narrowed around 0.5 for contrast, output range min..max)"),
    ("grow_organic", "input (mask node), pixels (default 6), irregularity (default 0.5), scale (whole number, default 8), seed", "grow a mask unevenly, like moss creeping out of a gap: Distance, then a Pixel Processor that moves the grow limit with a noise (and with a second image on `height` if given)"),
];

/// Adds one node of a macro (kind, params, inputs) and records it.
type AddNode<'a> = dyn FnMut(&mut Session, &str, Json, Json, &mut Vec<u32>) -> Result<u32, String> + 'a;

fn sd_macro_add(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let which = p.get("macro").and_then(Json::as_str).unwrap_or("").trim().to_lowercase();
    let o = p.get("options").cloned().unwrap_or_else(Json::obj);
    let num = |k: &str, d: f32| o.get(k).and_then(Json::as_f32).or_else(|| p.get(k).and_then(Json::as_f32)).unwrap_or(d);
    let input = match p.get("input").or_else(|| o.get("input")) {
        Some(v) => Some(s.node_ref(v).map_err(|e| format!("input: {e}"))?),
        None => None,
    };
    let need_input = || input.ok_or_else(|| format!("macro '{which}' needs input (the node it works on)"));
    let base = input.and_then(|i| s.graph.nodes.get(&i)).map(|n| n.pos).unwrap_or([0.0, 0.0]);
    let mut made: Vec<u32> = Vec::new();
    let mut col = 0.0f32;
    // add a node to the right of the last one
    let mut add = |s: &mut Session, kind: &str, params: Json, inputs: Json, made: &mut Vec<u32>| -> Result<u32, String> {
        col += 1.0;
        let q = Json::obj().with("kind", kind).with("params", params).with("inputs", inputs).with("x", base[0] + col * 170.0).with("y", base[1]);
        let out = sd_node_add(s, &q)?;
        let id = out.data.get("id").and_then(Json::as_u32).ok_or("node not created")?;
        made.push(id);
        Ok(id)
    };
    let link = |from: u32| Json::from(from);
    let grow = |s: &mut Session, src: u32, px: f32, made: &mut Vec<u32>, add: &mut AddNode| -> Result<u32, String> {
        let d = add(s, "sbs:distance", Json::obj().with("distance", (px * 2.0).max(1.0)), Json::obj().with("mask", link(src)), made)?;
        add(s, "sbs:levels", Json::obj().with("levelinlow", 0.47).with("levelinhigh", 0.53), Json::obj().with("input1", link(d)), made)
    };
    let invert = |s: &mut Session, src: u32, made: &mut Vec<u32>, add: &mut AddNode| -> Result<u32, String> {
        add(s, "sbs:levels", Json::obj().with("leveloutlow", 1.0).with("levelouthigh", 0.0), Json::obj().with("input1", link(src)), made)
    };
    let last = match which.as_str() {
        "grow" | "dilate" => {
            let src = need_input()?;
            grow(s, src, num("pixels", 4.0).clamp(0.5, 256.0), &mut made, &mut add)?
        }
        "shrink" | "erode" => {
            let src = need_input()?;
            let i1 = invert(s, src, &mut made, &mut add)?;
            let g = grow(s, i1, num("pixels", 4.0).clamp(0.5, 256.0), &mut made, &mut add)?;
            invert(s, g, &mut made, &mut add)?
        }
        "outline" => {
            let src = need_input()?;
            let px = num("pixels", 2.0).clamp(0.5, 128.0);
            let g = grow(s, src, px, &mut made, &mut add)?;
            let i1 = invert(s, src, &mut made, &mut add)?;
            let gi = grow(s, i1, px, &mut made, &mut add)?;
            let sh = invert(s, gi, &mut made, &mut add)?;
            add(s, "sbs:blend", Json::obj().with("blendingmode", "Subtract"), Json::obj().with("destination", link(g)).with("source", link(sh)), &mut made)?
        }
        "star" => {
            let pts = num("points", 5.0).round().clamp(3.0, 64.0);
            let inner = num("inner", 0.45).clamp(0.01, 1.0);
            let size = num("size", 0.45).clamp(0.01, 0.5);
            let rot = num("rotation", 0.0);
            let pp = add(s, "sbs:pixelprocessor", Json::obj().with("colorswitch", 0), Json::obj(), &mut made)?;
            // straight edges: in each half point the edge runs from the tip (radius `size`, angle 0) to the inner
            // corner (radius size * inner, angle pi / points); its radius at angle th follows the line through both
            let half = std::f32::consts::PI / pts;
            let ri = size * inner;
            let src = format!(
                "p = $pos - 0.5; r = length(p); a = atan2(p.y, p.x) / tau - {rot} + 0.25; \
                 th = (1 - abs(fract(a * {pts}) - 0.5) * 2) * {half}; \
                 edge = {k} / ({size} * sin(th) - {ri} * sin(th - {half})); saturate((edge - r) * $size.x + 0.5)",
                rot = fmt_f(rot),
                pts = fmt_f(pts),
                half = fmt_f(half),
                k = fmt_f(size * ri * half.sin()),
                size = fmt_f(size),
                ri = fmt_f(ri),
            );
            sd_node_expression(s, &Json::obj().with(NODE, pp).with("param", "perpixel").with("expression", src))?;
            pp
        }
        "polygon" => add(
            s,
            "sbs:lib:pattern_polygon_2/polygon_2",
            Json::obj().with("Sides", num("sides", 6.0).round().clamp(3.0, 32.0)).with("Scale", num("scale", 1.0)).with("Rotation", num("rotation", 0.0)).with("Gradient", num("bevel", 0.0).clamp(0.0, 1.0)),
            Json::obj(),
            &mut made,
        )?,
        "tile_shape" => {
            let src = need_input()?;
            let off = num("offset", 0.0);
            let sz = num("size", 1.0);
            add(
                s,
                "sbs:lib:pattern_tile_generator/tile_generator",
                Json::obj().with("pattern", "Image input").with("x_amount", num("x_amount", 4.0).round()).with("y_amount", num("y_amount", 4.0).round()).with("position_offset", off).with("pattern_size", Json::Arr(vec![Json::from(sz), Json::from(sz)])),
                Json::obj().with("pattern_input_1", link(src)),
                &mut made,
            )?
        }
        "strokes" => {
            let len = num("length", 0.9).clamp(0.05, 1.0);
            let wid = num("width", 0.2).clamp(0.01, 1.0);
            add(
                s,
                "sbs:lib:pattern_tile_sampler/tile_sampler",
                Json::obj()
                    .with("pattern", "Capsule")
                    .with("pattern_specific", 0.8)
                    .with("x_amount", num("x_amount", 8.0).round())
                    .with("y_amount", num("y_amount", 8.0).round())
                    .with("size", Json::Arr(vec![Json::from(len), Json::from(wid)]))
                    .with("rotation", num("angle", 0.0))
                    .with("rotation_random", num("angle_random", 0.1))
                    .with("position_random", num("position_random", 0.3))
                    .with("color_random", num("color_random", 0.3)),
                Json::obj(),
                &mut made,
            )?
        }
        "tile_scale" | "scale" => {
            let src = need_input()?;
            let k = num("times", 2.0).round().clamp(1.0, 64.0);
            add(s, "sbs:transformation", Json::obj().with("matrix22", Json::Arr(vec![Json::from(k), Json::from(0.0), Json::from(0.0), Json::from(k)])), Json::obj().with("input1", link(src)), &mut made)?
        }
        "bricks" | "brick" => {
            let rows = num("rows", 8.0).round().clamp(1.0, 128.0);
            let cols = (num("columns", 4.0) / 2.0).round().max(1.0) * 2.0;
            let sd = num("seed", 1.0);
            let mut e = format!(
                "R = {rows}; C = {cols}; row = floor($pos.y * R); fy = $pos.y * R - row; \
                 off = fract(row * {bond}); x = $pos.x * C + off * 1; cx = floor(x); fx = x - cx; \
                 pr = cellrand(vec2(cx % C, row), {sd}); \
                 cut = pr < {split} ? 1 : 0; hx = cut > 0 ? fract(fx * 2) : fx; hc = cut > 0 ? floor(fx * 2) : 0; \
                 pm = cellrand(vec2(floor(cx / 2) % (C / 2), row + 31), {sd}); \
                 joined = pm < {merge} ? 1 : 0; wx = joined > 0 ? (fract(cx / 2) * 2 + fx) * 0.5 : hx; \
                 ws = cut > 0 ? 0.5 : (joined > 0 ? 2 : 1); dx = min(wx, 1 - wx) * ws / C; dy = min(fy, 1 - fy) / R; d = min(dx, dy); ",
                rows = fmt_f(rows),
                cols = fmt_f(cols),
                bond = fmt_f(num("bond", 0.5)),
                sd = fmt_f(sd),
                split = fmt_f(num("split", 0.0)),
                merge = fmt_f(num("merge", 0.0)),
            );
            e += &expr_vnoise("cn", "$pos", 24.0, sd + 5.0);
            e += &format!(
                "mortar = {mortar} / R * 0.5; bev = {bev} / R + 0.0001; \
                 prof = smoothstep(0, 1, saturate((d - mortar) / bev)); \
                 hv = 1 - {hr} * cellrand(vec2(cx % C + hc * 7, row + 17), {sd} + 3); \
                 chip = 1 - {chips} * step(0.62, cn) * saturate(1 - (d - mortar) / (bev * 2)); \
                 prof * hv * chip",
                mortar = fmt_f(num("mortar", 0.08)),
                bev = fmt_f(num("bevel", 0.05)),
                hr = fmt_f(num("height_random", 0.2)),
                chips = fmt_f(num("chips", 0.3)),
            );
            let pp = add(s, "sbs:pixelprocessor", Json::obj().with("colorswitch", 0), Json::obj(), &mut made)?;
            sd_node_expression(s, &Json::obj().with(NODE, pp).with("param", "perpixel").with("expression", e))?;
            pp
        }
        "cracks" => {
            let sd = num("seed", 1.0);
            let mut e = expr_voronoi(num("cells", 6.0), sd);
            e += &expr_vnoise("cn", "$pos", num("cells", 6.0) * 2.0, sd + 9.0);
            e += &expr_vnoise("cm", "$pos", num("cells", 6.0) * 4.0, sd + 13.0);
            e += &format!(
                "w = {w} * saturate((cn - (1 - {reach})) / {reach_d}); e = (f2 - f1) / {n} * 0.5; \
                 w = w * (1 - {rough} + {rough} * cm * 2); \
                 saturate(1 - e / max(w, 0.0001))",
                w = fmt_f(num("width", 0.02)),
                n = fmt_f(num("cells", 6.0).round().max(1.0)),
                reach = fmt_f(num("reach", 0.5).clamp(0.05, 1.0)),
                reach_d = fmt_f(num("reach", 0.5).clamp(0.05, 1.0)),
                rough = fmt_f(num("roughness", 0.3).clamp(0.0, 1.0)),
            );
            let pp = add(s, "sbs:pixelprocessor", Json::obj().with("colorswitch", 0), Json::obj(), &mut made)?;
            sd_node_expression(s, &Json::obj().with(NODE, pp).with("param", "perpixel").with("expression", e))?;
            pp
        }
        "plates" | "fractured_rock" | "rock" => {
            let rock = which != "plates";
            let sd = num("seed", 1.0);
            let cells = num("cells", if rock { 5.0 } else { 6.0 });
            let mut e = expr_voronoi(cells, sd);
            e += &expr_vnoise("cn", "$pos", cells * 3.0, sd + 4.0);
            e += "ed = (f2 - f1) * 0.5; ";
            if rock {
                e += &format!(
                    "cr = saturate(1 - ed / {cw}); body = (1 - {hr} + {hr} * vid) * (1 - cr) * (1 - {er} * cn); saturate(body)",
                    cw = fmt_f(num("crack_width", 0.03) * 4.0),
                    hr = fmt_f(num("height_random", 0.4)),
                    er = fmt_f(num("erosion", 0.3)),
                );
            } else {
                e += &format!(
                    "edge = smoothstep({gap}, {gap} + {bev}, ed); tiltv = {tilt} * (0.5 - f1) * 0.5; \
                     saturate((1 - {hr} + {hr} * vid + tiltv) * edge)",
                    gap = fmt_f(num("gap", 0.03)),
                    bev = fmt_f(num("bevel", 0.15).max(0.01)),
                    tilt = fmt_f(num("dome", 0.3)),
                    hr = fmt_f(num("height_random", 0.5)),
                );
            }
            let pp = add(s, "sbs:pixelprocessor", Json::obj().with("colorswitch", 0), Json::obj(), &mut made)?;
            sd_node_expression(s, &Json::obj().with(NODE, pp).with("param", "perpixel").with("expression", e))?;
            pp
        }
        "moss" | "foliage" => {
            let sd = num("seed", 1.0);
            let n = num("density", 40.0).round().clamp(4.0, 256.0);
            let mut e = format!(
                "N = {n}; q = $pos * N; qi = floor(q); qf = fract(q); best = 0; ",
                n = fmt_f(n)
            );
            let mut prev = "best".to_string();
            let mut k = 0;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    k += 1;
                    e += &format!(
                        "m{k}c = vec2((qi.x + {dx}) % N, (qi.y + {dy}) % N); m{k}p = vec2({dx}, {dy}) + hash2(m{k}c + {sd}); m{k}r = {size} * (0.5 + 0.5 * cellrand(m{k}c, {sd} + 2)); \
                         m{k}v = saturate(1 - length(m{k}p - qf) / max(m{k}r, 0.01)); b{k} = max({prev}, m{k}v); ",
                        dx = fmt_f(dx as f32),
                        dy = fmt_f(dy as f32),
                        size = fmt_f(num("size", 0.9)),
                        sd = fmt_f(sd),
                    );
                    prev = format!("b{k}");
                }
            }
            e += &expr_vnoise("cl", "$pos", num("clump_scale", 4.0), sd + 11.0);
            e += &expr_vnoise("cl2", "$pos", num("clump_scale", 4.0) * 3.0, sd + 12.0);
            let clump = num("clumping", 0.6).clamp(0.0, 1.0);
            e += &format!(
                "mask = lerp(1, smoothstep(0.35, 0.65, cl * 0.7 + cl2 * 0.3), {clump}); saturate({prev} * mask)",
                clump = fmt_f(clump),
                prev = prev
            );
            let pp = add(s, "sbs:pixelprocessor", Json::obj().with("colorswitch", 0), Json::obj(), &mut made)?;
            sd_node_expression(s, &Json::obj().with(NODE, pp).with("param", "perpixel").with("expression", e))?;
            pp
        }
        "strata" | "layers" => {
            let sd = num("seed", 1.0);
            let layers = num("layers", 12.0).round().clamp(2.0, 128.0);
            let mut e = expr_vnoise("wv", "$pos", 3.0, sd + 7.0);
            e += &expr_vnoise("wv2", "$pos", 9.0, sd + 8.0);
            e += &format!(
                "y = $pos.y + (wv - 0.5) * {wav} * 0.2 + (wv2 - 0.5) * {wav} * 0.05 + {tilt} * sin(tau * $pos.x) / tau; \
                 L = {layers}; yl = y * L; li = floor(yl); lf = yl - li; \
                 hv = cellrand(vec2(li % L, 3), {sd}); \
                 sm = 1 - {edge} + {edge} * smoothstep(0, 0.35, lf) * (1 - smoothstep(0.65, 1, lf)); \
                 saturate((1 - {step} + {step} * hv) * sm)",
                wav = fmt_f(num("waviness", 0.3)),
                tilt = fmt_f(num("tilt", 0.1)),
                layers = fmt_f(layers),
                sd = fmt_f(sd),
                edge = fmt_f(num("edge", 0.1)),
                step = fmt_f(num("step", 0.5)),
            );
            let pp = add(s, "sbs:pixelprocessor", Json::obj().with("colorswitch", 0), Json::obj(), &mut made)?;
            sd_node_expression(s, &Json::obj().with(NODE, pp).with("param", "perpixel").with("expression", e))?;
            pp
        }
        "edge_chip" | "paint_chip" => {
            let paint = which == "paint_chip";
            let sd = num("seed", 1.0);
            let amount = num("amount", 0.5).clamp(0.0, 1.0);
            let scale = num("scale", if paint { 6.0 } else { 8.0 });
            // chip(pos): noise (and the input, if any) above a limit
            let chip_at = |pos: &str, pre: &str| -> String {
                let mut t = expr_vnoise(&format!("{pre}n"), pos, scale, sd);
                t += &expr_vnoise(&format!("{pre}m"), pos, scale * 3.0, sd + 5.0);
                if paint {
                    let base = if input.is_some() { format!("sample({pos})") } else { "0.5".to_string() };
                    t += &format!("{pre} = smoothstep(0.02, 0.0, {pre}n * 0.7 + {pre}m * 0.3 + (1 - {base}) * 0.4 - {lim}); ", lim = fmt_f(0.85 - amount * 0.5));
                } else {
                    t += &format!(
                        "{pre}e = saturate((abs(sample({pos} + vec2(1 / $size.x, 0)) - sample({pos} - vec2(1 / $size.x, 0))) + abs(sample({pos} + vec2(0, 1 / $size.y)) - sample({pos} - vec2(0, 1 / $size.y)))) * 8); \
                         {pre} = {pre}e * smoothstep({lim}, {lim} + 0.1, {pre}n * 0.7 + {pre}m * 0.3); ",
                        lim = fmt_f(1.0 - amount * 0.7)
                    );
                }
                t
            };
            if !paint {
                need_input()?;
            }
            let mut e = chip_at("$pos", "c");
            e += "c";
            let ins = match input {
                Some(i) => Json::obj().with("input", link(i)),
                None => Json::obj(),
            };
            let mask = add(s, "sbs:pixelprocessor", Json::obj().with("colorswitch", 0), ins.clone(), &mut made)?;
            sd_node_expression(s, &Json::obj().with(NODE, mask).with("param", "perpixel").with("expression", e))?;
            let mut outs = Json::obj().with("mask", mask);
            let last_id;
            if paint {
                // highlight toward the light, shadow away from it: the chip shifted against the chip
                let ang = num("light_angle", 0.125) * std::f32::consts::TAU;
                let th = num("thickness", 2.0).max(0.5);
                let off = format!("vec2({} , {}) * {} / $size.x", fmt_f(ang.cos()), fmt_f(-ang.sin()), fmt_f(th));
                let hi = add(s, "sbs:pixelprocessor", Json::obj().with("colorswitch", 0), Json::obj().with("input", link(mask)), &mut made)?;
                sd_node_expression(s, &Json::obj().with(NODE, hi).with("param", "perpixel").with("expression", format!("o = {off}; saturate(sample($pos - o) - sample($pos))")))?;
                let sh = add(s, "sbs:pixelprocessor", Json::obj().with("colorswitch", 0), Json::obj().with("input", link(mask)), &mut made)?;
                sd_node_expression(s, &Json::obj().with(NODE, sh).with("param", "perpixel").with("expression", format!("o = {off}; saturate(sample($pos + o) - sample($pos))")))?;
                outs = outs.with("highlight", hi).with("shadow", sh);
                last_id = mask;
            } else {
                let px = num("band", 2.0).max(0.5);
                let band = add(s, "sbs:pixelprocessor", Json::obj().with("colorswitch", 0), Json::obj().with("input", link(mask)), &mut made)?;
                let mut b = format!("r = {} / $size.x; m0 = sample($pos); ", fmt_f(px));
                let dirs = [(1.0f32, 0.0f32), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0), (0.7, 0.7), (-0.7, 0.7), (0.7, -0.7), (-0.7, -0.7)];
                let mut mx = "m0".to_string();
                for (k, (dx, dy)) in dirs.iter().enumerate() {
                    b += &format!("s{k} = sample($pos + vec2({}, {}) * r); ", fmt_f(*dx), fmt_f(*dy));
                    b += &format!("x{k} = max({mx}, s{k}); ");
                    mx = format!("x{k}");
                }
                b += &format!("saturate({mx} - m0)");
                sd_node_expression(s, &Json::obj().with(NODE, band).with("param", "perpixel").with("expression", b))?;
                outs = outs.with("band", band);
                last_id = mask;
            }
            if let Some(name) = p.get("name").and_then(Json::as_str) {
                if !name.is_empty() && name.parse::<u32>().is_err() {
                    s.aliases.insert(name.to_string(), last_id);
                }
            }
            return Outcome::data(Json::obj().with("macro", which).with("nodes", Json::Arr(made.iter().map(|i| Json::from(*i)).collect())).with("output_node", last_id).with("outputs", outs));
        }
        "flood_gray" | "flood_grey" => {
            let src = need_input()?;
            let lo = num("min", 0.0).clamp(0.0, 1.0);
            let hi = num("max", 1.0).clamp(0.0, 1.0);
            let c = num("contrast", 0.0).clamp(0.0, 0.95);
            let g = add(s, "sbs:lib:flood_fill_2/flood_fill_to_grayscale", Json::obj().with("luminance_random", 1.0).with("luminance_adjustement", 0.0).with("randomseed", num("seed", 0.0)), Json::obj().with("input", link(src)), &mut made)?;
            add(s, "sbs:levels", Json::obj().with("levelinlow", 0.5 * c).with("levelinhigh", 1.0 - 0.5 * c).with("leveloutlow", lo).with("levelouthigh", hi), Json::obj().with("input1", link(g)), &mut made)?
        }
        "grow_organic" | "organic_grow" => {
            let src = need_input()?;
            let sd = num("seed", 1.0);
            let px = num("pixels", 6.0).clamp(0.5, 256.0);
            let irr = num("irregularity", 0.5).clamp(0.0, 1.0);
            let d = add(s, "sbs:distance", Json::obj().with("distance", (px * 2.0).max(1.0)), Json::obj().with("mask", link(src)), &mut made)?;
            let mut e = expr_vnoise("gn", "$pos", num("scale", 8.0), sd);
            e += &expr_vnoise("gm", "$pos", num("scale", 8.0) * 3.0, sd + 3.0);
            e += &format!("thr = 0.5 - ({irr}) * ((gn * 0.7 + gm * 0.3) - 0.5) * 0.9; saturate((sample($pos) - thr) / 0.06 + 0.5)", irr = fmt_f(irr));
            let pp = add(s, "sbs:pixelprocessor", Json::obj().with("colorswitch", 0), Json::obj().with("input", link(d)), &mut made)?;
            sd_node_expression(s, &Json::obj().with(NODE, pp).with("param", "perpixel").with("expression", e))?;
            pp
        }
        "" | "list" => {
            let list: Vec<Json> = MACROS.iter().map(|(n, opts, what)| Json::obj().with("macro", *n).with("options", *opts).with("builds", *what)).collect();
            return Outcome::data(Json::obj().with("macros", Json::Arr(list)));
        }
        other => return Err(format!("unknown macro '{other}' ({})", MACROS.iter().map(|m| m.0).collect::<Vec<_>>().join(", "))),
    };
    if let Some(name) = p.get("name").and_then(Json::as_str) {
        if !name.is_empty() && name.parse::<u32>().is_err() {
            s.aliases.insert(name.to_string(), last);
        }
    }
    Outcome::data(Json::obj().with("macro", which).with("nodes", Json::Arr(made.iter().map(|i| Json::from(*i)).collect())).with("output_node", last))
}


/// Expression text for a tiling value noise (0..1) at `pos`, result in the local `pre`.
fn expr_vnoise(pre: &str, pos: &str, freq: f32, seed: f32) -> String {
    let f = fmt_f(freq.round().max(1.0));
    let sd = fmt_f(seed);
    format!(
        "{pre}q = ({pos}) * {f}; {pre}i = floor({pre}q); {pre}t = fract({pre}q); {pre}u = {pre}t * {pre}t * (3 - 2 * {pre}t); \
         {pre}a = cellrand(vec2({pre}i.x % {f}, {pre}i.y % {f}), {sd}); {pre}b = cellrand(vec2(({pre}i.x + 1) % {f}, {pre}i.y % {f}), {sd}); \
         {pre}c = cellrand(vec2({pre}i.x % {f}, ({pre}i.y + 1) % {f}), {sd}); {pre}d = cellrand(vec2(({pre}i.x + 1) % {f}, ({pre}i.y + 1) % {f}), {sd}); \
         {pre} = lerp(lerp({pre}a, {pre}b, {pre}u.x), lerp({pre}c, {pre}d, {pre}u.x), {pre}u.y); "
    )
}

/// Expression text for a tiling Voronoi net with `n` cells per side: locals `f1` (nearest), `f2` (second
/// nearest) distances in cell units and `vid` (a random number of the nearest cell, 0..1).
fn expr_voronoi(n: f32, seed: f32) -> String {
    let nn = fmt_f(n.round().max(1.0));
    let sd = fmt_f(seed);
    let mut t = format!("vq = $pos * {nn}; vi = floor(vq); vf = fract(vq); z1 = 9; z2 = 9; zi = 0; ");
    let mut prev = ("z1".to_string(), "z2".to_string(), "zi".to_string());
    let mut k = 0;
    for dy in -1..=1 {
        for dx in -1..=1 {
            k += 1;
            let cell = format!("vc{k}");
            t += &format!(
                "{cell} = vec2((vi.x + {dx}) % {nn}, (vi.y + {dy}) % {nn}); {cell}p = vec2({dx}, {dy}) + hash2({cell} + {sd}); d{k} = length({cell}p - vf); ",
                dx = fmt_f(dx as f32),
                dy = fmt_f(dy as f32)
            );
            let (a, b, c) = (format!("g1_{k}"), format!("g2_{k}"), format!("gi_{k}"));
            t += &format!(
                "{b} = min({p2}, max({p1}, d{k})); {c} = d{k} < {p1} ? cellrand({cell}, {sd}) : {pi}; {a} = min({p1}, d{k}); ",
                p1 = prev.0,
                p2 = prev.1,
                pi = prev.2
            );
            prev = (a, b, c);
        }
    }
    t += &format!("f1 = {}; f2 = {}; vid = {}; ", prev.0, prev.1, prev.2);
    t
}

fn fmt_f(x: f32) -> String {
    native::fmt_num(x)
}

fn sd_output_add(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let id = s.arg_node(p, NODE)?;
    let usage = p.get("usage").and_then(Json::as_str).unwrap_or("basecolor").to_string();
    let ident = p.get("name").and_then(Json::as_str).map(str::to_string);
    let out = p.get("output").and_then(Json::as_str).map(str::to_string);
    let sdu = native::sd_usage(&usage);
    // the same usage twice would clash when exporting
    let same: BTreeSet<u32> = s.graph.output_nodes().into_iter().filter(|(u, _, _)| native::sd_usage(u) == sdu).map(|o| o.2).collect();
    native::remove_nodes(&mut s.graph, &same);
    let pos = s.graph.nodes.get(&id).map(|n| [n.pos[0] + 170.0, n.pos[1]]).unwrap_or([0.0, 0.0]);
    let nid = native::add_output(&mut s.graph, &sdu, ident.as_deref(), pos);
    sd_set_link(&mut s.graph, nid, "inputNodeOutput", id, out)?;
    Outcome::data(Json::obj().with("output_node", nid).with("usage", sdu).with("replaced", same.len()))
}

fn sd_output_remove(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let usage = native::sd_usage(p.get("usage").and_then(Json::as_str).unwrap_or(""));
    let ids: BTreeSet<u32> = s.graph.output_nodes().into_iter().filter(|(u, i, _)| native::sd_usage(u) == usage || *i == usage).map(|o| o.2).collect();
    if ids.is_empty() {
        return Err(format!("there is no output with usage '{usage}'"));
    }
    native::remove_nodes(&mut s.graph, &ids);
    Outcome::data(Json::obj().with("removed", ids.len()))
}

fn sd_node_remove(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let key = if p.get("nodes").is_some() { "nodes" } else { "node" };
    let ids: BTreeSet<u32> = s.nodes_arg(p, key)?.into_iter().collect();
    native::remove_nodes(&mut s.graph, &ids);
    s.aliases.retain(|_, v| !ids.contains(v));
    Outcome::data(Json::obj().with("removed", ids.len()))
}

fn sd_node_duplicate(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let ids = s.nodes_arg(p, "nodes")?;
    let (dx, dy) = (p.get("dx").and_then(Json::as_f32).unwrap_or(40.0), p.get("dy").and_then(Json::as_f32).unwrap_or(40.0));
    let owner = s.graph.meta.get("identifier").cloned().unwrap_or_default();
    let mut map: BTreeMap<u32, u32> = BTreeMap::new();
    let mut next = native::free_id(&s.graph);
    for id in &ids {
        if matches!(s.graph.nodes.get(id).map(|n| n.kind.as_str()), Some("sbs:output" | "sbs:input")) {
            return Err(format!("node {id} is a graph input or output; add another with node.add / output.add"));
        }
        if !map.contains_key(id) {
            map.insert(*id, next);
            next += 1;
        }
    }
    for (old, new) in &map {
        let src = s.graph.nodes[old].clone();
        let mut n = native::copy_node(&src, &owner, *new, &mut s.graph.subgraphs);
        n.pos = [n.pos[0] + dx, n.pos[1] + dy];
        for v in n.ins.values_mut() {
            if let Some(m) = map.get(v) {
                *v = *m;
            }
        }
        s.graph.nodes.insert(*new, n);
    }
    Outcome::data(Json::obj().with("new_ids", Json::Arr(ids.iter().filter_map(|i| map.get(i)).map(|i| Json::from(*i)).collect())))
}

fn sd_node_set_result(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let id = s.arg_node(p, NODE)?;
    if sd_fam(&s.graph) == SFam::Texture {
        return Err("only function and FX-map graphs have a result node; texture graphs have outputs (output.add)".into());
    }
    let sd = s.graph.sd.as_mut().ok_or("not an .sbs graph")?;
    sd.root = Some(id);
    Outcome::data(Json::obj().with("result_node", id))
}

fn sd_graph_expose(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let id = s.arg_node(p, NODE)?;
    let label = p.get("label").and_then(Json::as_str).unwrap_or("").trim().to_string();
    let param = p.get("param").and_then(Json::as_str).unwrap_or("").to_string();
    let range = match (p.get("min").and_then(Json::as_f32), p.get("max").and_then(Json::as_f32)) {
        (Some(a), Some(b)) if b > a => Some((a, b)),
        (Some(_), Some(_)) => return Err("min must be smaller than max".into()),
        _ => None,
    };
    let ident: String = label.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    let ident = native::expose(&mut s.graph, id, &param, &ident, &label, range)?;
    Outcome::data(Json::obj().with("input", ident).with("note", "the graph now has this input; nodes running this graph can set it"))
}

fn sd_graph_unexpose(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let label = p.get("label").and_then(Json::as_str).unwrap_or("");
    let inputs = native::value_inputs(&s.graph);
    let ident = inputs.iter().find(|(i, l, _, _)| i == label || l == label).map(|x| x.0.clone()).ok_or_else(|| format!("the graph has no input '{label}'"))?;
    native::unexpose(&mut s.graph, &ident)?;
    Outcome::data(Json::obj().with("removed", ident))
}

/// The format setting of nodes: 0 8 bit, 1 16 bit, 2 16 bit float, 3 32 bit float.
fn format_code(v: &Json) -> Result<i32, String> {
    match v {
        Json::Num(_) => match v.as_f64() {
            Some(b) if b == 8.0 => Ok(0),
            Some(b) if b == 16.0 => Ok(1),
            Some(b) if b == 32.0 => Ok(3),
            _ => Err("default_format: 8, 16, \"16f\" or \"32f\"".into()),
        },
        Json::Str(t) => match t.trim().to_lowercase().as_str() {
            "8" | "8bit" | "8 bits" => Ok(0),
            "16" | "16bit" | "16 bits" => Ok(1),
            "16f" | "16 bits float" | "half" => Ok(2),
            "32f" | "32" | "32 bits float" | "float" => Ok(3),
            _ => Err("default_format: 8, 16, \"16f\" or \"32f\"".into()),
        },
        _ => Err("default_format: 8, 16, \"16f\" or \"32f\"".into()),
    }
}

fn sd_graph_set(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let sd = s.graph.sd.as_mut().ok_or("not an .sbs graph")?;
    let mut changed = 0;
    if p.get("size").is_some() || p.get("height").is_some() {
        let w = opt_usize(p, "size", 16, 8192, s.graph.size)?;
        let h = opt_usize(p, "height", 0, 8192, 0)?;
        let h = if h == 0 { w } else { h };
        for v in [w, h] {
            if !v.is_power_of_two() {
                return Err("sizes in .sbs packages are powers of two (16, 32 ... 8192)".into());
            }
        }
        let (lw, lh) = (w.trailing_zeros(), h.trailing_zeros());
        sd.base.insert("outputsize".into(), PVal::Const { ty: PTy::I(2), raw: format!("{lw} {lh}"), rel: Some("0".into()) }.enc());
        s.graph.size = w;
        s.graph.height = if h == w { 0 } else { h };
        changed += 1;
    }
    let sd = s.graph.sd.as_mut().ok_or("not an .sbs graph")?;
    if p.get("seed_offset").is_some() {
        let v = opt_usize(p, "seed_offset", 0, 1_000_000, 0)?;
        sd.base.insert("randomseed".into(), PVal::Const { ty: PTy::I(1), raw: v.to_string(), rel: Some("0".into()) }.enc());
        changed += 1;
    }
    if let Some(v) = p.get("output_format").filter(|v| !v.is_null()) {
        let code = match v.as_f64() {
            Some(b) if b == 8.0 => 0,
            Some(b) if b == 16.0 => 1,
            _ => return Err("output_format must be 8 or 16".into()),
        };
        sd.base.insert("format".into(), PVal::Const { ty: PTy::I(1), raw: code.to_string(), rel: Some("0".into()) }.enc());
        changed += 1;
    }
    if let Some(d) = p.get("details") {
        let o = d.as_obj().ok_or("details must be an object")?;
        for (k, v) in o {
            let key = match k.as_str() {
                "label" | "description" | "category" | "author" | "tags" => k.clone(),
                "author_url" => "authorURL".into(),
                other => return Err(format!("details: '{other}' is not kept by .sbs graphs (label, description, category, author, author_url, tags)")),
            };
            match v.as_str() {
                Some(t) if !t.is_empty() => {
                    s.graph.meta.insert(key, t.to_string());
                }
                _ => {
                    s.graph.meta.remove(&key);
                }
            }
            changed += 1;
        }
    }
    if let Some(v) = p.get("view") {
        let o = v.as_obj().ok_or("view must be an object (render.3d options: tiling, displacement, shape, env, yaw ...)")?;
        if o.is_empty() {
            s.graph.meta.remove("mc_view");
        } else {
            // the options must make a valid view
            view_from(v).map_err(|e| format!("view: {e}"))?;
            s.graph.meta.insert("mc_view".into(), v.to_text());
        }
        changed += 1;
    }
    if let Some(v) = p.get("default_format") {
        if v.is_null() {
            s.graph.meta.remove("mc_default_format");
        } else {
            let code = format_code(v)?;
            s.graph.meta.insert("mc_default_format".into(), code.to_string());
            let lib = s.graph.subgraphs.clone();
            let ids: Vec<u32> = s.graph.nodes.iter().filter(|(_, n)| !matches!(n.kind.as_str(), "sbs:output" | "sbs:input" | "sbs:graph" | "sbs:passthrough") && !n.kind.starts_with("sbs:fn:") && !n.kind.starts_with("sbs:fx:")).map(|(i, _)| *i).collect();
            for id in ids {
                if let Some(n) = s.graph.nodes.get_mut(&id) {
                    // nodes without a format setting are skipped
                    let _ = sd_apply(n, &lib, &[("format".to_string(), Json::from(code))], false);
                }
            }
        }
        changed += 1;
    }
    for k in ["precision", "normal_format"] {
        if p.get(k).is_some() {
            return Err(format!("{k} is not a graph setting of .sbs packages (nodes set it: format on each node, inversedy on Normal nodes)"));
        }
    }
    if changed == 0 {
        return Err("give at least one of size, height, seed_offset, output_format, default_format, view, details".into());
    }
    Outcome::data(Json::obj().with("graph", s.current_name()).with("size", s.graph.size))
}

fn sd_graph_list(s: &mut Session) -> Result<Outcome, String> {
    let root = &s.graph;
    let main = main_name(root);
    let mut list = Vec::new();
    for (name, g) in native::all_graphs(root) {
        if !native::listed(g) {
            continue;
        }
        let kind = if g.graph_kind() == "function" { "function" } else { "texture" };
        let mut j = Json::obj().with("name", name.as_str()).with("kind", kind).with("main", name == main).with("nodes", g.nodes.len());
        if s.focus.as_deref() == Some(name.as_str()) || (s.focus.is_none() && name == main) {
            j.set("focused", true);
        }
        if kind == "texture" {
            j.set("outputs", Json::Arr(g.output_idents().into_iter().map(Json::from).collect::<Vec<_>>()));
            let ins: Vec<Json> = native::value_inputs(g).into_iter().map(|(i, _, ty, _)| Json::from(format!("{i}: {}", ty.code()))).collect();
            if !ins.is_empty() {
                j.set("parameters", Json::Arr(ins));
            }
        }
        let users = native::graph_users(root, &name);
        if !users.is_empty() {
            j.set("used_by", Json::Arr(users.into_iter().map(Json::from).collect::<Vec<_>>()));
        }
        list.push(j);
    }
    let fns: Vec<Json> = root.subgraphs.iter().filter(|(_, g)| !native::listed(g)).map(|(k, _)| Json::from(k.as_str())).collect();
    let mut j = Json::obj().with("format", "sbs").with("count", list.len()).with("focused", s.focus.clone().unwrap_or(main)).with("graphs", Json::Arr(list));
    if !fns.is_empty() {
        j.set("value_functions", Json::Arr(fns));
        j.set("note", "value_functions compute values of single nodes (named <graph>/<node>/<parameter>); edit them with graph=<name>");
    }
    Outcome::data(j)
}

fn sd_graph_create(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let name = p.get("name").and_then(Json::as_str).unwrap_or("").trim().to_string();
    let kind = match p.get("kind").and_then(Json::as_str).unwrap_or("texture").trim().to_lowercase().as_str() {
        "" | "texture" | "graph" => "graph",
        "material" => "material",
        "function" | "fn" => "function",
        "pattern" | "fxmap" | "pattern program" => return Err("FX-map graphs live inside FX-Map nodes in .sbs packages (node.add kind=sbs:fxmaps)".into()),
        other => return Err(format!("unknown graph kind '{other}' (texture, material or function)")),
    };
    native::create_graph(&mut s.graph, &name, if kind == "material" { "graph" } else { kind })?;
    if kind == "material" {
        if let Some(g) = s.graph.subgraphs.get_mut(&name) {
            native::template_into(g, &[("baseColor", [0.5, 0.5, 0.5]), ("normal", [0.5, 0.5, 1.0]), ("roughness", [0.25, 0.25, 0.25]), ("metallic", [0.0, 0.0, 0.0]), ("height", [0.5, 0.5, 0.5]), ("ambientOcclusion", [1.0, 1.0, 1.0])]);
        }
    }
    if p.get("focus").and_then(Json::as_bool).unwrap_or(false) {
        s.focus = Some(name.clone());
    }
    let next = match kind {
        "function" => "add sbs:fn:* nodes with graph=<name>, then node.set_result; nodes call it with sbs:fn:instance",
        _ => "add nodes with graph=<name>; output.add makes outputs; other graphs use it with node.add kind=sbs:graph params={\"graph\": <name>}",
    };
    Outcome::data(Json::obj().with("created", name).with("kind", kind).with("next", next))
}

fn sd_graph_rename(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let old = p.get("graph").and_then(Json::as_str).unwrap_or("").to_string();
    let new = p.get("name").and_then(Json::as_str).unwrap_or("").trim().to_string();
    let old = match resolve_graph(&s.graph, &old)? {
        Some(o) => o,
        None => main_name(&s.graph),
    };
    let done = native::rename_graph(&mut s.graph, &old, &new)?;
    if let Some(a) = s.other_aliases.remove(&old) {
        s.other_aliases.insert(done.clone(), a);
    }
    if s.focus.as_deref() == Some(old.as_str()) {
        s.focus = Some(done.clone());
    }
    Outcome::data(Json::obj().with("renamed", done))
}

/// A new `.sbs` package: empty, or with the usual material outputs.
fn sd_new_package(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    let size = opt_usize(p, "size", 16, 8192, 512)?;
    if !size.is_power_of_two() {
        return Err("sizes in .sbs packages are powers of two (16, 32 ... 8192)".into());
    }
    let name = p.get("name").and_then(Json::as_str).unwrap_or("Substance_graph");
    let ident: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
    let mut g = native::new_package(&ident, size);
    let template = p.get("template").and_then(Json::as_str).unwrap_or("empty");
    match template {
        "empty" => {}
        "material" => native::template_into(&mut g, &[("baseColor", [0.5, 0.5, 0.5]), ("normal", [0.5, 0.5, 1.0]), ("roughness", [0.25, 0.25, 0.25]), ("metallic", [0.0, 0.0, 0.0]), ("height", [0.5, 0.5, 0.5]), ("ambientOcclusion", [1.0, 1.0, 1.0])]),
        other => return Err(format!("template '{other}': empty or material")),
    }
    s.graph = g;
    s.path = None;
    s.reset_names();
    Outcome::data(Json::obj().with("format", "sbs").with("graph", ident).with("size", size).with("template", template))
}


// ------------------------------------------------------------------------------------------ splines

/// Reference shapes for the editor and `spline.set`, as splines in the unit square.
pub fn spline_preset(name: &str) -> Option<crate::spline::Spline> {
    use crate::spline::{SPath, SPoint, Spline};
    let poly = |pts: &[(f32, f32)], smooth: bool| Spline { paths: vec![SPath { closed: true, smooth, pts: pts.iter().map(|&(x, y)| SPoint::new(x, y)).collect() }] };
    let ring = |n: usize, f: &dyn Fn(usize) -> f32, smooth: bool| {
        let pts: Vec<(f32, f32)> = (0..n)
            .map(|i| {
                let a = i as f32 / n as f32 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                let r = f(i);
                (0.5 + a.cos() * r, 0.5 + a.sin() * r)
            })
            .collect();
        poly(&pts, smooth)
    };
    Some(match name {
        "circle" => ring(8, &|_| 0.36, true),
        "square" => poly(&[(0.18, 0.18), (0.82, 0.18), (0.82, 0.82), (0.18, 0.82)], false),
        "rounded_square" => poly(&[(0.2, 0.2), (0.8, 0.2), (0.8, 0.8), (0.2, 0.8)], true),
        "triangle" => ring(3, &|_| 0.38, false),
        "star" => ring(10, &|i| if i % 2 == 0 { 0.4 } else { 0.17 }, false),
        "gear" => ring(24, &|i| if (i / 2) % 2 == 0 { 0.4 } else { 0.31 }, false),
        "heart" => poly(&[(0.5, 0.85), (0.15, 0.5), (0.2, 0.25), (0.38, 0.2), (0.5, 0.34), (0.62, 0.2), (0.8, 0.25), (0.85, 0.5)], true),
        "arrow" => poly(&[(0.15, 0.4), (0.55, 0.4), (0.55, 0.2), (0.88, 0.5), (0.55, 0.8), (0.55, 0.6), (0.15, 0.6)], false),
        "cross" => poly(&[(0.38, 0.15), (0.62, 0.15), (0.62, 0.38), (0.85, 0.38), (0.85, 0.62), (0.62, 0.62), (0.62, 0.85), (0.38, 0.85), (0.38, 0.62), (0.15, 0.62), (0.15, 0.38), (0.38, 0.38)], false),
        "wave" => Spline { paths: vec![SPath { closed: false, smooth: true, pts: (0..=8).map(|i| SPoint::new(i as f32 / 8.0, 0.5 + 0.18 * (i as f32 * std::f32::consts::FRAC_PI_2).sin())).collect() }] },
        "spiral" => Spline { paths: vec![SPath { closed: false, smooth: true, pts: (0..=16).map(|i| { let t = i as f32 / 16.0; let a = t * std::f32::consts::TAU * 2.5; let r = 0.04 + 0.34 * t; SPoint::new(0.5 + a.cos() * r, 0.5 + a.sin() * r) }).collect() }] },
        _ => return None,
    })
}

/// The resource name an SVG node reads (`pkg:///name?dependency=uid` -> `name`).
fn svg_node_resource(n: &crate::graph::Node) -> Option<String> {
    let v = native::values(n);
    let raw = match v.get("vectorresourcepath")? {
        PVal::Const { raw, .. } => raw.clone(),
        _ => return None,
    };
    let (path, _) = crate::sbs::split_path(&raw);
    Some(path.rsplit('/').next().unwrap_or(&path).to_string())
}

fn sd_spline_set(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    use crate::spline::{ShapeStyle, Spline};
    let sp = match (p.get("spline").and_then(Json::as_str), p.get("preset").and_then(Json::as_str)) {
        (Some(t), _) => Spline::parse(t)?,
        (None, Some(n)) => spline_preset(n).ok_or_else(|| format!("unknown preset '{n}' (circle, square, rounded_square, triangle, star, heart, arrow, gear, wave, spiral, cross)"))?,
        (None, None) => return Err("give a spline (text) or a preset".into()),
    };
    if sp.paths.is_empty() {
        return Err("the spline has no paths".into());
    }
    let mut styles: Vec<ShapeStyle> = vec![ShapeStyle::default(); sp.paths.len()];
    if let Some(Json::Arr(a)) = p.get("styles") {
        for (i, st) in a.iter().enumerate().take(styles.len()) {
            styles[i] = ShapeStyle { fill: st.get("fill").and_then(Json::as_bool).unwrap_or(true), stroke: st.get("stroke").and_then(Json::as_f32).unwrap_or(0.0).clamp(0.0, 0.5) };
        }
    }
    let existing = if p.get("node").is_some() { Some(s.arg_node(p, NODE)?) } else { None };
    if let Some(id) = existing {
        if s.graph.nodes.get(&id).map(|n| n.kind.as_str()) != Some("sbs:svg") {
            return Err("node is not an SVG node".into());
        }
    }
    let name = p
        .get("name")
        .and_then(Json::as_str)
        .map(str::to_string)
        .or_else(|| existing.and_then(|id| s.graph.nodes.get(&id)).and_then(svg_node_resource))
        .unwrap_or_else(|| "spline".into());
    let svg = sp.to_svg(&styles);
    let path = native::set_resource(&mut s.graph, &name, "svg", "svg", svg.into_bytes())?;
    if let Some(o) = s.outer.as_mut() {
        native::adopt_package(&mut o.root, &s.graph);
    }
    let id = match existing {
        Some(id) => id,
        None => {
            let pos = free_pos(&s.graph, &BTreeMap::new());
            let id = native::free_id(&s.graph);
            let mut n = native::new_node("sbs:svg", None, pos);
            native::set_value(&mut n, "colorswitch", Some(PVal::Const { ty: PTy::B, raw: "0".into(), rel: Some("0".into()) }));
            s.graph.nodes.insert(id, n);
            id
        }
    };
    if let Some(n) = s.graph.nodes.get_mut(&id) {
        native::set_value(n, "vectorresourcepath", Some(PVal::Const { ty: PTy::Path, raw: path.clone(), rel: Some("0".into()) }));
    }
    Outcome::data(Json::obj().with("node", id).with("resource", name.as_str()).with("path", path.as_str()).with("paths", sp.paths.len()).with("points", sp.point_count()))
}

fn sd_spline_get(s: &mut Session, p: &Json) -> Result<Outcome, String> {
    use crate::spline::Spline;
    let id = s.arg_node(p, NODE)?;
    let n = s.graph.nodes.get(&id).ok_or("no such node")?;
    let name = svg_node_resource(n).ok_or("the node shows no SVG resource")?;
    let bytes = s.graph.resources.iter().find(|(k, _)| k.starts_with(&format!("{name}."))).map(|(_, v)| v.clone()).ok_or_else(|| format!("resource '{name}' is not in the package"))?;
    let svg = String::from_utf8_lossy(&bytes).to_string();
    let (sp, styles) = Spline::from_svg_meta(&svg).ok_or("that SVG was not drawn with the spline editor")?;
    let st: Vec<Json> = styles.iter().map(|x| Json::obj().with("fill", x.fill).with("stroke", x.stroke)).collect();
    Outcome::data(Json::obj().with("resource", name.as_str()).with("spline", sp.to_text()).with("styles", Json::Arr(st)))
}
