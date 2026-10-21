// SPDX-License-Identifier: MIT OR Apache-2.0
//! Command line for the command interface: `mcp`, `serve`, `exec` and `commands`.
use crate::agent::{self, Backend, BridgeClient};
use crate::json::Json;
use crate::session::{self, Outcome, Session};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

pub const HELP: &str = "Drive MaterialCraft from scripts and AI assistants.

  materialcraft mcp [options]       MCP server on standard input / output (for Claude and other assistants)
  materialcraft serve [options]     keep a package in memory and accept commands on a local port
  materialcraft exec [options] <command> [name=value ...]    run one command
  materialcraft commands [words]    list the commands and their parameters

options:
  --graph FILE        open this package (.sbs) first
  --root DIR          limit every file the commands read or write to this folder
  --bridge [ADDR]     (mcp, exec) drive the running editor instead of an in-memory package; ADDR is 127.0.0.1:PORT,
                      left out it is found automatically
  --token TEXT        access token for --bridge ADDR (or set MATERIALCRAFT_CONTROL_TOKEN)
  --port N            (serve) port to listen on (default: any free port)
  --save [FILE]       (exec) write the package back afterwards (to FILE, or to the opened file)
  --out DIR           (exec) folder for pictures a command returns (default: the temporary folder)

values: name=3  name=true  name=\"text\"  name='{\"a\":1}'  - anything that is valid JSON is JSON, the rest is text.
A dotted name nests: params.scale=4 gives {\"params\":{\"scale\":4}}.

examples:
  materialcraft exec --graph wall.sbs render_node node=3 size=512
  materialcraft exec node_add kind=sbs:blur params.intensity=4 name=soft --graph wall.sbs --save
";

struct Opts {
    graph: Option<String>,
    root: Option<PathBuf>,
    bridge: Option<Option<String>>,
    token: Option<String>,
    port: u16,
    save: Option<Option<String>>,
    out: Option<PathBuf>,
    rest: Vec<String>,
}

fn parse_opts(args: &[String]) -> Result<Opts, String> {
    let mut o = Opts { graph: None, root: None, bridge: None, token: std::env::var("MATERIALCRAFT_CONTROL_TOKEN").ok().filter(|t| !t.is_empty()), port: 0, save: None, out: None, rest: Vec::new() };
    let mut i = 0;
    let next = |i: &mut usize, what: &str| -> Result<String, String> {
        *i += 1;
        args.get(*i).cloned().ok_or_else(|| format!("{what} needs a value"))
    };
    // an optional value: taken when the next word is not another option
    let maybe = |i: &mut usize| -> Option<String> {
        match args.get(*i + 1) {
            Some(v) if !v.starts_with("--") => {
                *i += 1;
                Some(v.clone())
            }
            _ => None,
        }
    };
    while i < args.len() {
        match args[i].as_str() {
            "--graph" => o.graph = Some(next(&mut i, "--graph")?),
            "--root" => {
                let r = PathBuf::from(next(&mut i, "--root")?);
                o.root = Some(std::fs::canonicalize(&r).map_err(|e| format!("{}: {e}", r.display()))?);
            }
            "--bridge" => {
                // --bridge takes an address only when it looks like one
                let a = match args.get(i + 1) {
                    Some(v) if !v.starts_with("--") && !v.contains('=') && (v.contains(':') || (!v.is_empty() && v.chars().all(|c| c.is_ascii_digit()))) => {
                        i += 1;
                        Some(v.clone())
                    }
                    _ => None,
                };
                o.bridge = Some(a);
            }
            "--token" => o.token = Some(next(&mut i, "--token")?),
            "--port" => o.port = next(&mut i, "--port")?.parse().map_err(|_| "--port needs a number")?,
            "--save" => o.save = Some(maybe(&mut i).filter(|v| !v.contains('=') && (v.ends_with(".sbs") || v.contains(['/', '\\'])))),
            "--out" => o.out = Some(PathBuf::from(next(&mut i, "--out")?)),
            "-h" | "--help" => {
                print!("{HELP}");
                return Err(String::new());
            }
            _ => o.rest.push(args[i].clone()),
        }
        i += 1;
    }
    Ok(o)
}

/// Keeps the files a bridged editor touches inside `--root`: the editor itself does not know about the limit.
struct Rooted<B: Backend> {
    inner: B,
    root: PathBuf,
}

impl<B: Backend> Backend for Rooted<B> {
    fn call(&mut self, command: &str, params: &Json) -> Result<Outcome, String> {
        let mut p = params.clone();
        if matches!(command.replace('_', ".").as_str(), "graph.open" | "graph.save" | "export") {
            for key in ["path", "dir"] {
                if let Some(v) = params.get(key).and_then(Json::as_str) {
                    p.set(key, session::confine(&self.root, v)?.display().to_string());
                }
            }
        }
        self.inner.call(command, &p)
    }
}

fn open_session(o: &Opts) -> Result<Session, String> {
    let mut s = Session::new();
    s.root = o.root.clone();
    if let Some(g) = &o.graph {
        s.execute("graph.open", &Json::obj().with("path", g.as_str()))?;
        s.dirty = false;
    }
    Ok(s)
}

fn token_from_discovery(port: u16) -> Result<String, String> {
    let need = "--bridge ADDR needs --token (or MATERIALCRAFT_CONTROL_TOKEN)".to_string();
    let text = std::fs::read_to_string(agent::discovery_file()).map_err(|_| need.clone())?;
    let j = Json::parse(&text)?;
    if j.get("port").and_then(Json::as_u32) == Some(port as u32) {
        j.get("token").and_then(Json::as_str).map(str::to_string).ok_or(need)
    } else {
        Err(need)
    }
}

fn bridge_to(addr: &Option<String>, token: &Option<String>) -> Result<BridgeClient, String> {
    match addr {
        Some(addr) => {
            let a = agent::parse_addr(addr)?;
            let token = match token {
                Some(t) => t.clone(),
                None => token_from_discovery(a.port())?,
            };
            Ok(BridgeClient::new(a, token))
        }
        None => BridgeClient::discover(),
    }
}

/// A bridge that looks for the editor when a command arrives, not at start-up: the assistant can be started before the
/// editor, and an editor that was restarted (new port and token) is found again.
struct LazyBridge {
    addr: Option<String>,
    token: Option<String>,
    inner: Option<BridgeClient>,
}

impl LazyBridge {
    fn new(o: &Opts) -> LazyBridge {
        LazyBridge { addr: o.bridge.clone().flatten(), token: o.token.clone(), inner: None }
    }
}

impl Backend for LazyBridge {
    fn call(&mut self, command: &str, params: &Json) -> Result<Outcome, String> {
        if self.inner.is_none() {
            self.inner = Some(bridge_to(&self.addr, &self.token).map_err(|e| format!("{e}. Open the MaterialCraft editor and turn on Tools > AI control, then try again"))?);
        }
        let r = self.inner.as_mut().map(|b| b.call(command, params)).unwrap_or_else(|| Err("not connected".into()));
        if let Err(e) = &r {
            // a lost editor, or one that has restarted with a new token, is looked for again on the next command
            if e.starts_with("cannot reach") || e.starts_with("connection lost") || e.starts_with("the editor closed") || e.contains("token") || e.contains("not authorized") {
                self.inner = None;
            }
        }
        r
    }
}

/// `name=value` words into a JSON object.
pub fn parse_params(words: &[String]) -> Result<Json, String> {
    let mut root = Json::obj();
    for w in words {
        let (k, v) = w.split_once('=').ok_or_else(|| format!("'{w}' should look like name=value"))?;
        if k.is_empty() {
            return Err(format!("'{w}' has no name"));
        }
        let val = match v.chars().next() {
            Some('{' | '[' | '"') => Json::parse(v).map_err(|e| format!("{k}: {e}"))?,
            _ => match v {
                "true" => Json::Bool(true),
                "false" => Json::Bool(false),
                "null" => Json::Null,
                _ => match Json::parse(v) {
                    Ok(n @ Json::Num(_)) => n,
                    _ => Json::Str(v.to_string()),
                },
            },
        };
        let parts: Vec<&str> = k.split('.').collect();
        insert_path(&mut root, &parts, val);
    }
    Ok(root)
}

fn insert_path(obj: &mut Json, path: &[&str], val: Json) {
    match path {
        [] => {}
        [last] => obj.set(last, val),
        [first, rest @ ..] => {
            let mut child = obj.get(first).cloned().filter(|c| matches!(c, Json::Obj(_))).unwrap_or_else(Json::obj);
            insert_path(&mut child, rest, val);
            obj.set(first, child);
        }
    }
}

fn print_outcome(o: &Outcome, dir: Option<&PathBuf>) -> Result<(), String> {
    println!("{}", o.data.to_pretty());
    let dir = dir.cloned().unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for (i, im) in o.images.iter().enumerate() {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        let name: String = im.label.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
        let p = dir.join(format!("materialcraft_{name}_{stamp}_{i}.png"));
        crate::safeio::write_file(&p, &im.png).map_err(|e| format!("{}: {e}", p.display()))?;
        println!("image: {}", p.display());
    }
    Ok(())
}

pub fn run(cmd: &str, args: &[String]) -> Result<(), String> {
    let o = match parse_opts(args) {
        Ok(o) => o,
        Err(e) if e.is_empty() => return Ok(()),
        Err(e) => return Err(e),
    };
    match cmd {
        "commands" => {
            let f = o.rest.join(" ").to_lowercase();
            for c in session::COMMANDS {
                if !f.is_empty() && !c.id.contains(&f) && !c.doc.to_lowercase().contains(&f) {
                    continue;
                }
                println!("{:<18} {}", c.id, c.doc);
                for a in c.args {
                    println!("    {}{:<12} {:<8} {}", if a.req { '*' } else { ' ' }, a.name, a.ty, a.doc);
                }
            }
            Ok(())
        }
        "mcp" => {
            eprintln!("materialcraft MCP server {} ready (standard input / output)", env!("CARGO_PKG_VERSION"));
            let r = if o.bridge.is_some() {
                let b = LazyBridge::new(&o);
                match &o.root {
                    Some(root) => agent::serve_mcp(&mut Rooted { inner: b, root: root.clone() }),
                    None => {
                        let mut b = b;
                        agent::serve_mcp(&mut b)
                    }
                }
            } else {
                let mut s = open_session(&o)?;
                agent::serve_mcp(&mut s)
            };
            r.map_err(|e| e.to_string())
        }
        "serve" => {
            let s = Arc::new(Mutex::new(open_session(&o)?));
            let h = s.clone();
            let handler: Arc<agent::Handler> = Arc::new(move |c, p| h.lock().unwrap_or_else(PoisonError::into_inner).execute(c, p));
            let srv = agent::start_control(o.port, o.token.clone(), handler).map_err(|e| format!("cannot listen: {e}"))?;
            let file = agent::write_discovery(srv.port, &srv.token).map_err(|e| format!("cannot write the discovery file: {e}"))?;
            eprintln!("listening on 127.0.0.1:{}  (connection details: {})", srv.port, file.display());
            eprintln!("start `materialcraft mcp --bridge` to let an assistant use this graph; press Ctrl+C to stop");
            loop {
                std::thread::sleep(std::time::Duration::from_secs(3600));
            }
        }
        "exec" => {
            let (name, words) = o.rest.split_first().ok_or("usage: materialcraft exec [options] <command> [name=value ...]")?;
            let params = parse_params(words)?;
            if o.bridge.is_some() {
                let mut b = LazyBridge::new(&o);
                let r = match &o.root {
                    Some(root) => Rooted { inner: b, root: root.clone() }.call(name, &params)?,
                    None => b.call(name, &params)?,
                };
                return print_outcome(&r, o.out.as_ref());
            }
            let mut s = open_session(&o)?;
            let r = s.execute(name, &params)?;
            print_outcome(&r, o.out.as_ref())?;
            if let Some(target) = &o.save {
                let mut p = Json::obj();
                if let Some(t) = target {
                    p.set("path", t.as_str());
                }
                let saved = s.execute("graph.save", &p)?;
                eprintln!("{}", saved.data.to_text());
            }
            Ok(())
        }
        _ => Err(format!("unknown command '{cmd}'")),
    }
}
