// SPDX-License-Identifier: MIT OR Apache-2.0
// ======================================================================= AI control (included into gui.rs)
//
// The editor can listen on a local port so that an assistant (through `materialcraft mcp --bridge`) or a script can
// edit the graph that is open: it adds nodes, connects them, sets values and renders, and the editor shows every
// change as it happens. Commands that change the graph run here on the interface thread, as one undo step each; the
// commands that only look (renders, statistics) run on the connection's own thread on a copy of the graph, so a slow
// render never freezes the window.

use materialcraft::agent;
use materialcraft::json::Json;
use materialcraft::session::{self, Kind, Outcome, Session};
use std::sync::mpsc::Sender;
use std::sync::PoisonError;

/// Largest time an assistant waits for the window to answer.
const CTL_WAIT: std::time::Duration = std::time::Duration::from_secs(120);

enum CtlReq {
    Snapshot(Sender<CtlSnap>),
    Exec { cmd: String, params: Json, reply: Sender<Result<Outcome, String>> },
}

struct CtlSnap {
    graph: Graph,
    path: Option<PathBuf>,
    dirty: bool,
    book: session::AliasBook,
    focus: Option<String>,
}

struct ControlLink {
    rx: Receiver<CtlReq>,
    server: agent::ControlServer,
    /// node names of every graph of the package, and the graph commands work on by default
    book: session::AliasBook,
    focus: Option<String>,
    commands: usize,
}

/// The command line of the `materialcraft` program that sits next to this one (it hosts `mcp`).
fn cli_path() -> String {
    let exe = std::env::current_exe().ok();
    let name = if cfg!(windows) { "materialcraft.exe" } else { "materialcraft" };
    exe.and_then(|e| e.parent().map(|d| d.join(name))).filter(|p| p.exists()).map(|p| p.display().to_string()).unwrap_or_else(|| "materialcraft".into())
}

fn mcp_config_json() -> String {
    let c = cli_path();
    Json::obj().with("mcpServers", Json::obj().with("materialcraft", Json::obj().with("command", c).with("args", Json::Arr(vec!["mcp".into(), "--bridge".into()])))).to_pretty()
}

fn claude_code_command() -> String {
    let c = cli_path();
    if c.contains(' ') {
        format!("claude mcp add materialcraft -- \"{c}\" mcp --bridge")
    } else {
        format!("claude mcp add materialcraft -- {c} mcp --bridge")
    }
}

impl App {
    fn ctl_start(&mut self, ctx: &egui::Context, port: u16) {
        if self.ctl.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel::<CtlReq>();
        let tx = Mutex::new(tx);
        let cache = Arc::new(NodeCache::with_default_budget());
        let repaint = ctx.clone();
        let handler: Arc<agent::Handler> = Arc::new(move |cmd: &str, params: &Json| -> Result<Outcome, String> {
            let c = session::find_tool(cmd).ok_or_else(|| format!("unknown command '{cmd}' (command.list shows them all)"))?;
            let on_ui = c.kind != Kind::Query || matches!(c.id, "graph.save" | "edit.history");
            let send = |m: CtlReq| -> Result<(), String> {
                tx.lock().unwrap_or_else(PoisonError::into_inner).send(m).map_err(|_| "the editor is no longer listening".to_string())?;
                repaint.request_repaint();
                Ok(())
            };
            if on_ui {
                let (rtx, rrx) = std::sync::mpsc::channel();
                send(CtlReq::Exec { cmd: cmd.to_string(), params: params.clone(), reply: rtx })?;
                return rrx.recv_timeout(CTL_WAIT).map_err(|_| "the editor did not answer in time (is a dialog open?)".to_string())?;
            }
            let (rtx, rrx) = std::sync::mpsc::channel();
            send(CtlReq::Snapshot(rtx))?;
            let snap = rrx.recv_timeout(CTL_WAIT).map_err(|_| "the editor did not answer in time (is a dialog open?)".to_string())?;
            Session::from_package(snap.graph, snap.path, snap.dirty, snap.book, snap.focus, cache.clone()).execute(cmd, params)
        });
        match agent::start_control(port, None, handler) {
            Ok(server) => {
                match agent::write_discovery(server.port, &server.token) {
                    Ok(_) => self.status(format!("AI control is on (port {})", server.port), true),
                    Err(e) => self.status(format!("AI control is on (port {}) but its connection file could not be written: {e}", server.port), false),
                }
                self.ctl = Some(ControlLink { rx, server, book: BTreeMap::new(), focus: None, commands: 0 });
            }
            Err(e) => self.status(format!("AI control could not start: {e}"), false),
        }
    }

    fn ctl_stop(&mut self) {
        if self.ctl.take().is_some() {
            self.status("AI control is off", true);
        }
    }

    /// Answer the requests assistants have sent since the last frame.
    fn poll_control(&mut self, ctx: &egui::Context) {
        let mut work: Vec<CtlReq> = Vec::new();
        if let Some(l) = &self.ctl {
            while work.len() < 32 {
                match l.rx.try_recv() {
                    Ok(m) => work.push(m),
                    Err(_) => break,
                }
            }
        }
        if work.is_empty() {
            return;
        }
        for m in work {
            match m {
                CtlReq::Snapshot(reply) => {
                    let (book, focus) = self.ctl.as_ref().map(|l| (l.book.clone(), l.focus.clone())).unwrap_or_default();
                    let _ = reply.send(CtlSnap { graph: self.g.clone(), path: self.file.clone(), dirty: self.modified, book, focus });
                }
                CtlReq::Exec { cmd, params, reply } => {
                    // a panic while editing must never take the window down
                    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.ctl_exec(&cmd, &params))).unwrap_or_else(|_| Err(format!("internal error in {cmd}")));
                    if let Some(l) = &mut self.ctl {
                        l.commands += 1;
                    }
                    let _ = reply.send(r);
                }
            }
        }
        ctx.request_repaint();
    }

    /// Show the editor (a new, empty graph when the window is still on the start page).
    fn ctl_ensure_editor(&mut self) {
        if self.screen == Screen::Home {
            if self.pkgs.is_empty() {
                // a new .sbs package, as File > New makes
                let g = materialcraft::sbs_native::new_package("Substance_graph", 512);
                let name = self.unique_unsaved_name();
                self.ws_fresh();
                self.load_active(Pkg { path: None, name, g, modified: false, tabs: Vec::new() });
            }
            self.screen = Screen::Editor;
        }
    }

    fn ctl_exec(&mut self, cmd: &str, params: &Json) -> Result<Outcome, String> {
        let c = session::find_tool(cmd).ok_or_else(|| format!("unknown command '{cmd}'"))?;
        self.ctl_ensure_editor();
        let ok = |j: Json| Ok(Outcome { data: j, images: Vec::new() });
        match c.id {
            "edit.undo" => {
                if self.undo.is_empty() {
                    return Err("nothing to undo".into());
                }
                self.undo();
                return ok(Json::obj().with("undone", true).with("undo_steps", self.undo.len()).with("redo_steps", self.redo.len()));
            }
            "edit.redo" => {
                if self.redo.is_empty() {
                    return Err("nothing to redo".into());
                }
                self.redo();
                return ok(Json::obj().with("redone", true).with("undo_steps", self.undo.len()).with("redo_steps", self.redo.len()));
            }
            "edit.history" => return ok(Json::obj().with("undo_steps", self.undo.len()).with("redo_steps", self.redo.len())),
            "graph.save" => {
                self.goto_root();
                let path = match params.get("path").and_then(Json::as_str) {
                    Some(p) => PathBuf::from(p),
                    None => self.file.clone().ok_or("this graph has no file yet: give a path")?,
                };
                self.save_to(path.clone());
                if self.modified {
                    return Err(self.status.0.clone());
                }
                return ok(Json::obj().with("saved", path.display().to_string()));
            }
            // these open the result as a new package tab, like File > New / Open / the Library
            "graph.new" | "graph.open" | "library.load" => {
                let mut tmp = Session::new();
                let r = tmp.execute(c.id, params)?;
                let mut g = tmp.graph.clone();
                g.migrate_outputs();
                auto_layout_if_unplaced(&mut g);
                if !g.meta.get("identifier").map(|s| !s.is_empty()).unwrap_or(false) {
                    g.meta.insert("identifier".into(), "Graph".into());
                }
                let (path, name) = match c.id {
                    "graph.open" => {
                        let p = tmp.path.clone();
                        let n = match &p {
                            Some(p) => p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
                            // an imported .sbs package: unsaved, named after the file
                            None => r.data.get("file").and_then(Json::as_str).map(|f| format!("{} (imported)", std::path::Path::new(f).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default())).unwrap_or_else(|| "Imported".into()),
                        };
                        (p, n)
                    }
                    "library.load" => (None, r.data.get("loaded").and_then(Json::as_str).unwrap_or("Library graph").to_string()),
                    _ => (None, self.unique_unsaved_name()),
                };
                let pk = Pkg { path, name, g, modified: false, tabs: Vec::new() };
                self.add_pkg_active(pk);
                self.need_fit = true;
                if let Some(l) = &mut self.ctl {
                    l.book.clear();
                    l.focus = None;
                }
                return Ok(r);
            }
            _ => {}
        }
        // everything else runs on a copy of the graph; when it changed, the copy replaces the graph as one undo step
        let (book, focus) = self.ctl.as_ref().map(|l| (l.book.clone(), l.focus.clone())).unwrap_or_default();
        let mut s = Session::from_package(self.g.clone(), self.file.clone(), self.modified, book, focus, Arc::new(NodeCache::new(256)));
        let r = s.execute(c.id, params);
        if r.is_ok() && s.take_changed() {
            let before: BTreeSet<u32> = self.g.nodes.keys().copied().collect();
            let was_small = self.g.nodes.len() <= 1;
            self.snap();
            if let Some(l) = &mut self.ctl {
                l.book = s.alias_book();
                l.focus = s.focus().map(str::to_string);
            }
            self.g = s.into_graph();
            self.after_restore();
            let fresh: BTreeSet<u32> = self.g.nodes.keys().copied().filter(|i| !before.contains(i)).collect();
            if let Some(last) = fresh.iter().next_back() {
                self.sel = fresh.clone();
                self.active = Some(*last);
            }
            if was_small && self.g.nodes.len() > 1 {
                self.need_fit = true;
            }
            self.status(format!("AI: {cmd}"), true);
        } else if let Some(l) = &mut self.ctl {
            l.book = s.alias_book();
            l.focus = s.focus().map(str::to_string);
        }
        r
    }

    fn ai_window(&mut self, ctx: &egui::Context) {
        if !self.show_ai {
            return;
        }
        let mut open = true;
        egui::Window::new("AI control").open(&mut open).collapsible(false).resizable(false).default_width(470.0).show(ctx, |ui| {
            ui.label("Let an AI assistant (or a script) build and edit the graph that is open here. You watch every step and can undo it with Ctrl+Z.");
            ui.add_space(4.0);
            match &self.ctl {
                Some(l) => {
                    ui.colored_label(Color32::from_rgb(120, 200, 120), format!("Listening on 127.0.0.1:{}  ({} commands received)", l.server.port, l.commands));
                    ui.small("Only programs on this computer that know the access token can connect. The token changes every time you turn this on.");
                    ui.add_space(6.0);
                    ui.label("Claude Code: run this once in a terminal");
                    let cmd = claude_code_command();
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut cmd.as_str()).desired_width(380.0));
                        if ui.button("Copy").clicked() {
                            ctx.copy_text(cmd.clone());
                        }
                    });
                    ui.label("Other assistants: add this to their MCP settings");
                    let cfg = mcp_config_json();
                    ui.add(egui::TextEdit::multiline(&mut cfg.as_str()).desired_rows(7).desired_width(440.0).font(egui::TextStyle::Monospace));
                    if ui.button("Copy settings").clicked() {
                        ctx.copy_text(cfg);
                    }
                    ui.add_space(6.0);
                    if ui.button("Turn off").clicked() {
                        self.ctl_stop();
                    }
                }
                None => {
                    ui.label("Off. Nothing can reach the editor.");
                    if ui.button("Turn on").clicked() {
                        self.ctl_start(ctx, 0);
                    }
                }
            }
        });
        self.show_ai = open;
    }
}
