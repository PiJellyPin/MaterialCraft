// SPDX-License-Identifier: MIT OR Apache-2.0
//! Ways in for other programs and AI assistants: an MCP server on standard input / output, a token-protected
//! JSON-lines control channel on the local machine, and a client that lets the MCP server drive a running editor.
//!
//! Everything arrives through [`crate::session::Session::execute`] (or an editor that implements [`Backend`]), so a
//! command does the same thing from the command line, from an assistant and from a script.
//!
//! The shape of this follows FilmCraft's automation layer (MIT OR Apache-2.0, storytold/filmcraft): one command
//! registry, a JSON-lines control port on 127.0.0.1, and an MCP server that either runs headless or bridges to the
//! open application. MaterialCraft adds a required access token, because the port can change the user's files.

use crate::graph::{b64_decode, b64_encode};
use crate::json::Json;
use crate::session::{self, Image, Outcome, Session};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

/// Longest request line accepted (MCP and control channel).
pub const MAX_LINE: usize = 16 * 1024 * 1024;
/// Connections the control channel serves at once.
pub const MAX_CONNECTIONS: usize = 8;
/// Newest MCP protocol revision this server speaks; older clients are answered with their own revision.
const MCP_VERSION: &str = "2025-06-18";
const MCP_KNOWN: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18"];

/// Something that runs commands: a headless session, or the open editor reached through the control channel.
pub trait Backend {
    fn call(&mut self, command: &str, params: &Json) -> Result<Outcome, String>;
}

impl Backend for Session {
    fn call(&mut self, command: &str, params: &Json) -> Result<Outcome, String> {
        self.execute(command, params)
    }
}

impl Backend for Arc<Mutex<Session>> {
    fn call(&mut self, command: &str, params: &Json) -> Result<Outcome, String> {
        self.lock().unwrap_or_else(PoisonError::into_inner).execute(command, params)
    }
}

// ------------------------------------------------------------------------------------------ token + discovery

/// 256 random bits as hex: from the operating system when possible, otherwise from the randomly keyed hasher.
pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    let mut have = false;
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        have = f.read_exact(&mut bytes).is_ok();
    }
    if !have {
        use std::hash::{BuildHasher, Hasher};
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        for chunk in bytes.chunks_mut(8) {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u128(t);
            h.write_u32(std::process::id());
            chunk.copy_from_slice(&h.finish().to_le_bytes());
        }
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn same(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        diff |= (a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0)) as usize;
    }
    diff == 0
}

/// Where a running editor or `serve` process writes how to reach it (port and token), readable only by the user.
pub fn discovery_file() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library").join("Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    };
    base.unwrap_or_else(std::env::temp_dir).join("MaterialCraft").join("control.json")
}

fn write_private(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    let mut f = o.open(path)?;
    f.write_all(text.as_bytes())
}

pub fn write_discovery(port: u16, token: &str) -> std::io::Result<PathBuf> {
    let p = discovery_file();
    write_private(&p, &Json::obj().with("port", port as u32).with("token", token).with("pid", std::process::id()).to_pretty())?;
    Ok(p)
}

pub fn remove_discovery(token: &str) {
    let p = discovery_file();
    if let Ok(t) = std::fs::read_to_string(&p) {
        if Json::parse(&t).ok().and_then(|j| j.get("token").and_then(Json::as_str).map(|x| x == token)).unwrap_or(false) {
            let _ = std::fs::remove_file(p);
        }
    }
}

// ------------------------------------------------------------------------------------------ replies

fn image_json(i: &Image) -> Json {
    Json::obj().with("label", i.label.as_str()).with("mime", "image/png").with("data", b64_encode(&i.png))
}

fn outcome_json(o: &Outcome) -> Json {
    let mut j = Json::obj().with("data", o.data.clone());
    if !o.images.is_empty() {
        j.set("images", Json::Arr(o.images.iter().map(image_json).collect()));
    }
    j
}

fn outcome_from_json(j: &Json) -> Outcome {
    let images = j
        .get("images")
        .and_then(Json::as_arr)
        .unwrap_or(&[])
        .iter()
        .filter_map(|i| Some(Image { label: i.get("label").and_then(Json::as_str).unwrap_or("").to_string(), png: b64_decode(i.get("data")?.as_str()?).ok()? }))
        .collect();
    Outcome { data: j.get("data").cloned().unwrap_or(Json::Null), images }
}

// ------------------------------------------------------------------------------------------ line reading

/// Read one line of at most `MAX_LINE` bytes. `Ok(None)` at end of input; an over-long line is an error.
fn read_line<R: BufRead>(r: &mut R) -> std::io::Result<Option<String>> {
    let mut buf: Vec<u8> = Vec::new();
    loop {
        let chunk = r.fill_buf()?;
        if chunk.is_empty() {
            return Ok(if buf.is_empty() { None } else { Some(String::from_utf8_lossy(&buf).into_owned()) });
        }
        match chunk.iter().position(|b| *b == b'\n') {
            Some(i) => {
                buf.extend_from_slice(&chunk[..i]);
                r.consume(i + 1);
                break;
            }
            None => {
                let n = chunk.len();
                buf.extend_from_slice(chunk);
                r.consume(n);
            }
        }
        if buf.len() > MAX_LINE {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "line too long"));
        }
    }
    if buf.len() > MAX_LINE {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "line too long"));
    }
    if buf.last() == Some(&b'\r') {
        buf.pop();
    }
    Ok(Some(String::from_utf8_lossy(&buf).into_owned()))
}

// ------------------------------------------------------------------------------------------ MCP server

const INSTRUCTIONS: &str = "MaterialCraft builds procedural PBR materials as graphs of nodes in a Substance package (.sbs). Work like this:\n\
1. node_types (search by words) and node_describe show what a node does, its inputs, outputs and every parameter with default, range, choices (numbered settings: set them by number or by name, e.g. \"pattern\": \"Bell\") and units (angles are turns: 0.25 = 90 degrees). Ranges are slider ranges: larger values are accepted.\n\
2. Build with `batch`: one call, all-or-nothing, one undo step. Give nodes a `name` in node_add and use that name instead of the numeric id in later steps. Connect with node_add `inputs` ({slot: node}) or node_connect.\n\
3. A material needs outputs: output_add with usage basecolor, normal, roughness, metallic, height, ambient_occlusion, opacity, emissive. Opacity and emissive are supported.\n\
4. Look at your work often: render_review is the main check (one picture: lit 3D preview, every map, the node graph with a thumbnail per node; data: the graph as text, statistics, problems; with reference=<image> it compares brightness, colour, contrast, detail and feature size and says what to change). Then render_node for one step (picture + statistics; tile=2 checks seams; Flood Fill nodes report their region count), render_3d (lit object; list=true shows every view option: environments, light rigs, tiling, displacement ...), graph_outline (the graph as text), graph_validate (problems, and warnings such as nodes whose result is not used).\n\
5. Textures must tile: render_node reports tiles_seamlessly; seam_horizontal / seam_vertical at or below about 1 are good (above 1.5 is a visible seam). Zoom patterns by whole numbers (macro_add tile_scale).\n\
6. Math as text: node_expression turns an expression into a function graph, for any parameter or a Pixel Processor (param perpixel; sample($pos) reads its input), e.g. \"p = $pos - 0.5; length(p) < 0.3 ? 1 : 0\".\n\
7. macro_add inserts tested groups of standard nodes: grow / shrink / outline a mask, star (any number of points), polygon, tile_shape (your shape repeated as tiles), strokes (directional strokes per cell), tile_scale. macro=list shows their options.\n\
8. Several graphs: graph_list shows the package's graphs; every graph and node command takes graph=<name> (or graph_focus picks one for the following commands). graph_create makes a new graph (kind texture, material or function); graph_extract moves chosen nodes into a new graph and puts one node running it in their place (a subgraph); node_add kind=sbs:graph params={\"graph\": <name>} runs a graph inside another.\n\
9. graph_new starts a package (template=material gives the usual outputs); graph_save / graph_open read and write .sbs packages. edit_undo undoes the last change.\n\
Node ids are numbers; names you chose also work. Parameters are exact: a wrong name is rejected with suggestions.";

fn rpc_error(id: &Json, code: i32, msg: &str) -> Json {
    Json::obj().with("jsonrpc", "2.0").with("id", id.clone()).with("error", Json::obj().with("code", code).with("message", msg))
}

fn rpc_result(id: &Json, r: Json) -> Json {
    Json::obj().with("jsonrpc", "2.0").with("id", id.clone()).with("result", r)
}

fn tool_list() -> Json {
    let mut tools = Vec::new();
    for c in session::COMMANDS {
        let mut t = Json::obj().with("name", c.tool_name()).with("description", c.doc).with("inputSchema", c.schema());
        let read_only = c.kind == session::Kind::Query && !matches!(c.id, "graph.save" | "export");
        t.set("annotations", Json::obj().with("readOnlyHint", read_only).with("destructiveHint", false).with("openWorldHint", false));
        tools.push(t);
    }
    Json::obj().with("tools", Json::Arr(tools))
}

/// Answer one MCP message. `None` for notifications (no reply).
pub fn mcp_handle(backend: &mut dyn Backend, msg: &Json) -> Option<Json> {
    let id = msg.get("id").cloned();
    let method = match msg.get("method").and_then(Json::as_str) {
        Some(m) => m,
        None => return id.map(|i| rpc_error(&i, -32600, "invalid request: no method")),
    };
    let Some(id) = id else {
        return None; // a notification such as notifications/initialized
    };
    let params = msg.get("params").cloned().unwrap_or(Json::Null);
    Some(match method {
        "initialize" => {
            let want = params.get("protocolVersion").and_then(Json::as_str).unwrap_or(MCP_VERSION);
            let version = if MCP_KNOWN.contains(&want) { want } else { MCP_VERSION };
            rpc_result(
                &id,
                Json::obj()
                    .with("protocolVersion", version)
                    .with("capabilities", Json::obj().with("tools", Json::obj().with("listChanged", false)))
                    .with("serverInfo", Json::obj().with("name", "materialcraft").with("version", env!("CARGO_PKG_VERSION")))
                    .with("instructions", INSTRUCTIONS),
            )
        }
        "ping" => rpc_result(&id, Json::obj()),
        "tools/list" => rpc_result(&id, tool_list()),
        "tools/call" => {
            let name = match params.get("name").and_then(Json::as_str) {
                Some(n) => n,
                None => return Some(rpc_error(&id, -32602, "tools/call needs a name")),
            };
            let args = params.get("arguments").cloned().unwrap_or(Json::Null);
            let r = if session::find_tool(name).is_none() { Err(format!("unknown tool '{name}'")) } else { backend.call(name, &args) };
            let (content, is_error) = match r {
                Ok(o) => {
                    let mut c = vec![Json::obj().with("type", "text").with("text", o.data.to_pretty())];
                    for im in &o.images {
                        c.push(Json::obj().with("type", "image").with("data", b64_encode(&im.png)).with("mimeType", "image/png"));
                    }
                    (c, false)
                }
                Err(e) => (vec![Json::obj().with("type", "text").with("text", e)], true),
            };
            rpc_result(&id, Json::obj().with("content", Json::Arr(content)).with("isError", is_error))
        }
        "resources/list" => rpc_result(&id, Json::obj().with("resources", Json::Arr(Vec::new()))),
        "prompts/list" => rpc_result(&id, Json::obj().with("prompts", Json::Arr(Vec::new()))),
        _ => rpc_error(&id, -32601, &format!("method not found: {method}")),
    })
}

/// Run the MCP server on standard input / output until the client closes it. Standard output carries only protocol
/// messages; diagnostics go to standard error.
pub fn serve_mcp(backend: &mut dyn Backend) -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let mut r = BufReader::new(stdin.lock());
    let stdout = std::io::stdout();
    loop {
        let line = match read_line(&mut r) {
            Ok(Some(l)) => l,
            Ok(None) => return Ok(()),
            Err(e) => {
                let mut o = stdout.lock();
                writeln!(o, "{}", rpc_error(&Json::Null, -32700, &format!("{e}")).to_text())?;
                o.flush()?;
                return Ok(());
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match Json::parse(&line) {
            Err(e) => Some(rpc_error(&Json::Null, -32700, &e)),
            Ok(Json::Arr(items)) => {
                let out: Vec<Json> = items.iter().filter_map(|m| mcp_handle(backend, m)).collect();
                (!out.is_empty()).then_some(Json::Arr(out))
            }
            Ok(m @ Json::Obj(_)) => mcp_handle(backend, &m),
            Ok(_) => Some(rpc_error(&Json::Null, -32600, "invalid request")),
        };
        if let Some(rep) = reply {
            let mut o = stdout.lock();
            writeln!(o, "{}", rep.to_text())?;
            o.flush()?;
        }
    }
}

// ------------------------------------------------------------------------------------------ control channel

/// What the control channel needs from the program that owns the graph.
pub type Handler = dyn Fn(&str, &Json) -> Result<Outcome, String> + Send + Sync;

fn control_reply(id: &Json, r: Result<Json, String>) -> String {
    let mut j = Json::obj();
    if !id.is_null() {
        j.set("id", id.clone());
    }
    match r {
        Ok(v) => {
            j.set("ok", true);
            j.set("result", v);
        }
        Err(e) => {
            j.set("ok", false);
            j.set("error", e);
        }
    }
    j.to_text()
}

fn serve_connection(stream: TcpStream, token: &str, handler: &Handler) {
    let _ = stream.set_nodelay(true);
    // a connection must prove it knows the token quickly, or it is dropped
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let Ok(w) = stream.try_clone() else { return };
    let mut w = w;
    let mut r = BufReader::new(stream);
    let mut authed = false;
    loop {
        let line = match read_line(&mut r) {
            Ok(Some(l)) => l,
            _ => return,
        };
        if line.trim().is_empty() {
            continue;
        }
        // anything that is not a JSON object with a string "method" closes the connection (a web page cannot smuggle
        // commands in the body of an HTTP request)
        let msg = match Json::parse(&line) {
            Ok(m @ Json::Obj(_)) if m.get("method").and_then(Json::as_str).is_some() => m,
            _ => {
                let _ = writeln!(w, "{}", control_reply(&Json::Null, Err("expected a JSON object with a \"method\"; closing the connection".into())));
                return;
            }
        };
        let id = msg.get("id").cloned().unwrap_or(Json::Null);
        let method = msg.get("method").and_then(Json::as_str).unwrap_or("");
        let params = msg.get("params").cloned().unwrap_or(Json::Null);
        if !authed {
            let ok = method == "auth" && params.get("token").and_then(Json::as_str).map(|t| same(t, token)).unwrap_or(false);
            if !ok {
                let _ = writeln!(w, "{}", control_reply(&id, Err("authentication required: send {\"method\":\"auth\",\"params\":{\"token\":\"...\"}} first".into())));
                return;
            }
            authed = true;
            let _ = r.get_ref().set_read_timeout(None);
            if writeln!(w, "{}", control_reply(&id, Ok(Json::obj().with("authenticated", true).with("version", env!("CARGO_PKG_VERSION"))))).is_err() {
                return;
            }
            continue;
        }
        let res = match method {
            "auth" => Ok(Json::obj().with("authenticated", true)),
            "ping" => Ok(Json::obj().with("pong", true)),
            "engine.commands" => handler("command.list", &Json::Null).map(|o| o.data),
            "engine.execute" => match params.get("command").and_then(Json::as_str) {
                Some(c) => handler(c, params.get("params").unwrap_or(&Json::Null)).map(|o| outcome_json(&o)),
                None => Err("engine.execute needs a \"command\"".into()),
            },
            m => Err(format!("unknown method '{m}' (methods: auth, ping, engine.commands, engine.execute)")),
        };
        if writeln!(w, "{}", control_reply(&id, res)).is_err() {
            return;
        }
    }
}

/// A running control channel.
pub struct ControlServer {
    pub port: u16,
    pub token: String,
    stop: Arc<AtomicBool>,
    conns: Arc<Mutex<Vec<TcpStream>>>,
}

impl ControlServer {
    /// Stop accepting connections and close the ones that are open.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        for c in self.conns.lock().unwrap_or_else(PoisonError::into_inner).drain(..) {
            let _ = c.shutdown(std::net::Shutdown::Both);
        }
    }
}

impl Drop for ControlServer {
    fn drop(&mut self) {
        self.stop();
        remove_discovery(&self.token);
    }
}

/// Start the control channel on 127.0.0.1 (`port` 0 = any free port). Runs on background threads; the returned
/// token must accompany every connection. The port and token are also written to [`discovery_file`].
pub fn start_control(port: u16, token: Option<String>, handler: Arc<Handler>) -> std::io::Result<ControlServer> {
    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))?;
    listener.set_nonblocking(true)?;
    let port = listener.local_addr()?.port();
    let token = token.unwrap_or_else(random_token);
    let active = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let conns: Arc<Mutex<Vec<TcpStream>>> = Arc::new(Mutex::new(Vec::new()));
    let (tok, stop2, conns2) = (token.clone(), stop.clone(), conns.clone());
    std::thread::Builder::new().name("control-accept".into()).spawn(move || {
        while !stop2.load(Ordering::SeqCst) {
            let stream = match listener.accept() {
                Ok((s, _)) => s,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(40));
                    continue;
                }
                Err(_) => {
                    std::thread::sleep(Duration::from_millis(200));
                    continue;
                }
            };
            let _ = stream.set_nonblocking(false);
            if active.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
                active.fetch_sub(1, Ordering::SeqCst);
                let mut s = stream;
                let _ = writeln!(s, "{}", control_reply(&Json::Null, Err("too many connections".into())));
                continue;
            }
            if let Ok(c) = stream.try_clone() {
                let mut list = conns2.lock().unwrap_or_else(PoisonError::into_inner);
                list.retain(|x| x.peer_addr().is_ok());
                list.push(c);
            }
            let (h, t, a) = (handler.clone(), tok.clone(), active.clone());
            let spawned = std::thread::Builder::new().name("control-conn".into()).spawn(move || {
                // a panic in a command must not take the program down or leave the slot taken
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| serve_connection(stream, &t, &*h)));
                a.fetch_sub(1, Ordering::SeqCst);
            });
            if spawned.is_err() {
                active.fetch_sub(1, Ordering::SeqCst);
            }
        }
    })?;
    Ok(ControlServer { port, token, stop, conns })
}

// ------------------------------------------------------------------------------------------ bridge client

/// Talks to a control channel: the other end of `--bridge`.
pub struct BridgeClient {
    addr: SocketAddr,
    token: String,
    conn: Option<(BufReader<TcpStream>, TcpStream)>,
    next_id: u32,
}

impl BridgeClient {
    pub fn new(addr: SocketAddr, token: String) -> BridgeClient {
        BridgeClient { addr, token, conn: None, next_id: 1 }
    }

    /// Find a running editor through the discovery file.
    pub fn discover() -> Result<BridgeClient, String> {
        let p = discovery_file();
        let t = std::fs::read_to_string(&p).map_err(|_| format!("no running MaterialCraft found ({} does not exist). Start the editor with --control, or use `materialcraft serve`", p.display()))?;
        let j = Json::parse(&t).map_err(|e| format!("{}: {e}", p.display()))?;
        let port = j.get("port").and_then(Json::as_u32).and_then(|p| u16::try_from(p).ok()).ok_or("the discovery file has no port")?;
        let token = j.get("token").and_then(Json::as_str).ok_or("the discovery file has no token")?.to_string();
        Ok(BridgeClient::new(SocketAddr::from((Ipv4Addr::LOCALHOST, port)), token))
    }

    fn connect(&mut self) -> Result<(), String> {
        let s = TcpStream::connect_timeout(&self.addr, Duration::from_secs(5)).map_err(|e| format!("cannot reach MaterialCraft at {}: {e}", self.addr))?;
        let _ = s.set_nodelay(true);
        let w = s.try_clone().map_err(|e| e.to_string())?;
        self.conn = Some((BufReader::new(s), w));
        let t = self.token.clone();
        self.request("auth", Json::obj().with("token", t)).map(|_| ())
    }

    fn request(&mut self, method: &str, params: Json) -> Result<Json, String> {
        let (r, w) = self.conn.as_mut().ok_or("not connected")?;
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        let req = Json::obj().with("id", id).with("method", method).with("params", params);
        writeln!(w, "{}", req.to_text()).map_err(|e| format!("connection lost: {e}"))?;
        let _ = w.flush();
        let line = read_line(r).map_err(|e| format!("connection lost: {e}"))?.ok_or("the editor closed the connection")?;
        let rep = Json::parse(&line)?;
        if rep.get("ok").and_then(Json::as_bool) == Some(true) {
            Ok(rep.get("result").cloned().unwrap_or(Json::Null))
        } else {
            Err(rep.get("error").and_then(Json::as_str).unwrap_or("request failed").to_string())
        }
    }
}

impl Backend for BridgeClient {
    fn call(&mut self, command: &str, params: &Json) -> Result<Outcome, String> {
        let body = || Json::obj().with("command", command).with("params", params.clone());
        if self.conn.is_none() {
            self.connect()?;
        }
        // a dropped connection (the editor was restarted) is retried once, with a fresh login
        let r = match self.request("engine.execute", body()) {
            Err(e) if e.starts_with("connection lost") || e.starts_with("the editor closed") => {
                self.conn = None;
                self.connect()?;
                self.request("engine.execute", body())
            }
            other => other,
        };
        r.map(|j| outcome_from_json(&j))
    }
}

/// Parse `host:port`, `:port` or `port` (the host must be this computer).
pub fn parse_addr(s: &str) -> Result<SocketAddr, String> {
    let t = s.trim();
    let port: u16 = t.rsplit(':').next().unwrap_or(t).parse().map_err(|_| format!("'{s}' is not an address like 127.0.0.1:9877"))?;
    let host = t.rsplit_once(':').map(|x| x.0).unwrap_or("");
    if !(host.is_empty() || host == "127.0.0.1" || host == "localhost") {
        return Err("only this computer (127.0.0.1) can be controlled".into());
    }
    Ok(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
}
