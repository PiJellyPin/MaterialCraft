// SPDX-License-Identifier: MIT OR Apache-2.0
//! Templates the user makes from their own graphs: a graph saved (with the graphs and functions it uses) as an `.sbs`
//! package in a templates folder, offered by the New dialog next to the built-in templates. A new project made from
//! one is a copy of that package with the graph renamed.
use crate::graph::Graph;
use crate::sbs_native;
use std::path::{Path, PathBuf};

/// A saved template: its name (the file name without `.sbs`) and its file.
#[derive(Debug, Clone, PartialEq)]
pub struct UserTemplate {
    pub name: String,
    pub path: PathBuf,
}

/// The name a template is saved under: letters, digits, spaces, `-` and `_` (others become `_`), trimmed.
pub fn clean_name(name: &str) -> String {
    name.trim().chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_') { c } else { '_' }).collect::<String>().trim().to_string()
}

/// The file template `name` is saved in.
pub fn path_for(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{}.sbs", clean_name(name)))
}

/// Save graph `graph` of package `root` as template `name` in `dir` (replacing a template of that name). Returns
/// its file.
pub fn save(dir: &Path, name: &str, root: &Graph, graph: &str) -> Result<PathBuf, String> {
    if clean_name(name).is_empty() {
        return Err("the template needs a name".into());
    }
    let text = sbs_native::extract_package(root, graph).and_then(|g| sbs_native::save_text(&g))?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let p = path_for(dir, name);
    crate::safeio::write_file(&p, text.as_bytes()).map_err(|e| format!("{}: {e}", p.display()))?;
    Ok(p)
}

/// The templates in `dir`, by name (none when the folder does not exist yet).
pub fn list(dir: &Path) -> Vec<UserTemplate> {
    let mut v: Vec<UserTemplate> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.is_file() && p.extension().map(|x| x.eq_ignore_ascii_case("sbs")).unwrap_or(false))
                .filter_map(|p| Some(UserTemplate { name: p.file_stem()?.to_string_lossy().into_owned(), path: p }))
                .collect()
        })
        .unwrap_or_default();
    v.sort_by_key(|t| t.name.to_ascii_lowercase());
    v
}

/// The output identifiers of a template's graph (for the New dialog's details).
pub fn outputs(t: &UserTemplate) -> Result<Vec<String>, String> {
    let (g, _) = sbs_native::open(&t.path)?;
    Ok(g.output_idents())
}

/// A new package from template `t`, its graph renamed `graph` (letters, digits and `_`).
pub fn instantiate(t: &UserTemplate, graph: &str) -> Result<Graph, String> {
    let (mut g, _) = sbs_native::open(&t.path)?;
    let old = sbs_native::graph_name(&g);
    let new: String = graph.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
    if !new.is_empty() && new != old {
        sbs_native::rename_graph(&mut g, &old, &new)?;
    }
    Ok(g)
}
