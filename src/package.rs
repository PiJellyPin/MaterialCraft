// SPDX-License-Identifier: MIT OR Apache-2.0
//! The graphs of a package: which graphs a graph uses, a graph ready to evaluate on its own, and renaming, removing
//! and copying graphs with every node that names them following (.sbs packages hand these to `sbs_native`).

use crate::graph::Graph;
use std::collections::{BTreeMap, BTreeSet};

impl Graph {
    /// Names of the graphs this graph uses, directly or through them (graph nodes, pattern programs, functions
    /// that drive settings or run processors): every text setting that names one of `lib`'s graphs.
    pub fn graphs_used(&self, lib: &BTreeMap<String, Graph>) -> BTreeSet<String> {
        let mut used = BTreeSet::new();
        let mut todo: Vec<&Graph> = vec![self];
        while let Some(g) = todo.pop() {
            for n in g.nodes.values() {
                for v in n.strs.values() {
                    if let Some(sub) = lib.get(v) {
                        if used.insert(v.clone()) {
                            todo.push(sub);
                        }
                    }
                }
            }
        }
        used
    }

    /// Graph `name` of this package ready to evaluate on its own: it sees the package's other graphs and resources
    /// (what the editor shows when the graph is opened). `None` is the package's main graph.
    pub fn graph_view(&self, name: Option<&str>) -> Result<Graph, String> {
        let Some(name) = name.filter(|n| !n.is_empty()) else { return Ok(self.clone()) };
        let mut w = self.subgraphs.get(name).cloned().ok_or_else(|| format!("the package has no graph '{name}'"))?;
        w.subgraphs = self.subgraphs.clone();
        w.subgraphs.remove(name);
        if self.sd.is_some() {
            // graphs of an .sbs package may use the package's first graph too
            let mut me = self.clone();
            me.subgraphs.clear();
            w.subgraphs.entry(crate::sbs_native::graph_name(self)).or_insert(me);
        }
        w.resources = self.resources.clone();
        w.seed_offset = self.seed_offset;
        if !w.meta.get("identifier").map(|s| !s.is_empty()).unwrap_or(false) {
            w.meta.insert("identifier".into(), name.to_string());
        }
        Ok(w)
    }
}

/// Text settings that name another graph of the package: graph nodes (`graph`), pattern maps (`program`),
/// processors and function calls (`fn`) and function-driven settings (`fn.<setting>`).
pub fn is_graph_ref(key: &str) -> bool {
    matches!(key, "graph" | "program" | "fn") || key.starts_with("fn.")
}

/// A graph name a package can use: letters, digits and `_`, not empty.
pub fn valid_graph_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 128 && name.chars().all(|c| c.is_alphanumeric() || c == '_')
}

impl Graph {
    /// The package's graphs that use graph `name`.
    pub fn graph_users(&self, name: &str) -> Vec<String> {
        if self.sd.is_some() {
            return crate::sbs_native::graph_users(self, name);
        }
        let mut users = Vec::new();
        let main = self.meta.get("identifier").cloned().filter(|s| !s.is_empty()).unwrap_or_else(|| "main graph".into());
        let uses = |g: &Graph| g.nodes.values().any(|n| n.strs.iter().any(|(k, v)| is_graph_ref(k) && v == name));
        if uses(self) {
            users.push(main);
        }
        for (k, g) in &self.subgraphs {
            if k != name && uses(g) {
                users.push(k.clone());
            }
        }
        users
    }

    /// Rename graph `old` of this package to `new`; every node that names it (graph nodes, programs, functions)
    /// follows.
    pub fn rename_graph(&mut self, old: &str, new: &str) -> Result<(), String> {
        if self.sd.is_some() {
            return crate::sbs_native::rename_graph(self, old, new).map(|_| ());
        }
        if !self.subgraphs.contains_key(old) {
            return Err(format!("the package has no graph '{old}'"));
        }
        if !valid_graph_name(new) {
            return Err(format!("'{new}' is not a valid graph name (letters, digits and _)"));
        }
        if old == new {
            return Ok(());
        }
        let main = self.meta.get("identifier").cloned().unwrap_or_default();
        if self.subgraphs.contains_key(new) || main == new {
            return Err(format!("the package already has a graph called '{new}'"));
        }
        let mut g = self.subgraphs.remove(old).unwrap_or_default();
        if g.meta.get("identifier").map(|i| i == old).unwrap_or(false) {
            g.meta.insert("identifier".into(), new.to_string());
        }
        self.subgraphs.insert(new.to_string(), g);
        let fix = |g: &mut Graph| {
            for n in g.nodes.values_mut() {
                for (k, v) in n.strs.iter_mut() {
                    if is_graph_ref(k) && v == old {
                        *v = new.to_string();
                    }
                }
            }
        };
        fix(self);
        for g in self.subgraphs.values_mut() {
            fix(g);
        }
        Ok(())
    }

    /// Remove graph `name` from the package; refused while another graph still uses it.
    pub fn remove_graph(&mut self, name: &str) -> Result<(), String> {
        if self.sd.is_some() {
            return crate::sbs_native::remove_graph(self, name);
        }
        if !self.subgraphs.contains_key(name) {
            return Err(format!("the package has no graph '{name}'"));
        }
        let users = self.graph_users(name);
        if !users.is_empty() {
            return Err(format!("'{name}' is still used by {}", users.join(", ")));
        }
        self.subgraphs.remove(name);
        Ok(())
    }

    /// Copy graph `name` (`None`: the main graph) as a new graph of the package; returns the new name.
    pub fn duplicate_graph(&mut self, name: Option<&str>, new: Option<&str>) -> Result<String, String> {
        if self.sd.is_some() {
            return crate::sbs_native::duplicate_graph(self, name, new);
        }
        let mut g = match name {
            Some(n) => self.subgraphs.get(n).cloned().ok_or_else(|| format!("the package has no graph '{n}'"))?,
            None => {
                let mut g = self.clone();
                g.subgraphs.clear();
                g.resources.clear();
                g
            }
        };
        let base = new.map(str::to_string).unwrap_or_else(|| format!("{}_copy", name.map(str::to_string).unwrap_or_else(|| self.meta.get("identifier").cloned().unwrap_or_else(|| "graph".into()))));
        if !valid_graph_name(&base) {
            return Err(format!("'{base}' is not a valid graph name (letters, digits and _)"));
        }
        let main = self.meta.get("identifier").cloned().unwrap_or_default();
        let mut unique = base.clone();
        let mut k = 2;
        while self.subgraphs.contains_key(&unique) || unique == main {
            if new.is_some() {
                return Err(format!("the package already has a graph called '{base}'"));
            }
            unique = format!("{base}_{k}");
            k += 1;
        }
        g.meta.insert("identifier".into(), unique.clone());
        g.meta.remove("embedded");
        self.subgraphs.insert(unique.clone(), g);
        Ok(unique)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Graph {
        let text = "size 256\nmeta identifier \"wall\"\nsubgraph wear\nnode 1 perlin scale=4\nnode 2 output usage=\"out\" in=@1\nend\nsubgraph spare\nnode 1 white_noise\nnode 2 output usage=\"out\" in=@1\nend\nnode 1 subgraph graph=\"wear\" _x=10 _y=20\nnode 2 white_noise\nnode 3 blend mode=3 fg=@1 bg=@2\nnode 4 output usage=\"basecolor\" in=@3\n";
        Graph::parse_unchecked(text).unwrap()
    }

    #[test]
    fn graphs_used_and_views() {
        let g = sample();
        let used = g.graphs_used(&g.subgraphs);
        assert!(used.contains("wear") && !used.contains("spare"));
        let v = g.graph_view(Some("wear")).unwrap();
        assert_eq!(v.meta.get("identifier").map(String::as_str), Some("wear"));
        assert!(v.subgraphs.contains_key("spare") && !v.subgraphs.contains_key("wear"));
        assert!(g.graph_view(Some("nope")).is_err());
    }

    #[test]
    fn graphs_are_renamed_removed_and_copied_with_their_users() {
        let mut g = sample();
        assert_eq!(g.graph_users("wear"), vec!["wall".to_string()]);
        assert!(g.remove_graph("wear").is_err());
        g.rename_graph("wear", "worn").unwrap();
        assert_eq!(g.nodes[&1].strs["graph"], "worn");
        assert!(g.rename_graph("worn", "spare").is_err());
        assert!(g.rename_graph("worn", "bad name").is_err());
        g.remove_graph("spare").unwrap();
        let c = g.duplicate_graph(Some("worn"), None).unwrap();
        assert_eq!(c, "worn_copy");
        let m = g.duplicate_graph(None, Some("wall_v2")).unwrap();
        assert!(g.subgraphs[&m].subgraphs.is_empty() && g.subgraphs[&m].nodes.len() == 4);
    }
}
