//! `hub <app>`: one stdio MCP server that fronts every `mode: "hub"` tool.
//!
//! - `tools/list` is answered from cached manifests; no backend starts.
//! - The first `tools/call` for a backend spawns it (with only its secrets),
//!   performs the MCP handshake and forwards the call.
//! - Idle backends are stopped; config edits are picked up live and announced
//!   with `notifications/tools/list_changed`.

use std::collections::{BTreeMap, HashMap};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime};

use serde_json::{Value, json};

use crate::config::{self, Config, Mode, Tool};
use crate::paths::Paths;
use crate::run;

const DEFAULT_PROTOCOL: &str = "2025-06-18";
const DEFAULT_IDLE: u64 = 300;
const SEP: &str = "__";

type Out = Arc<Mutex<std::io::Stdout>>;
/// "Start in progress" flag plus its wake-up signal.
type Gate = Arc<(Mutex<bool>, Condvar)>;

fn send(out: &Out, value: &Value) {
    let mut line = value.to_string();
    line.push('\n');
    let mut lock = out.lock().unwrap();
    let _ = lock.write_all(line.as_bytes());
    let _ = lock.flush();
}

fn error(id: &Value, code: i64, message: impl Into<String>) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message.into()}})
}

fn short_id(tool: &Tool) -> String {
    tool.id.strip_prefix("agent-").unwrap_or(&tool.id).replace('-', "_")
}

fn manifest_path(paths: &Paths, id: &str) -> PathBuf {
    paths.cache_dir.join("tools").join(format!("{id}.json"))
}

fn load_manifest(paths: &Paths, id: &str) -> Option<Value> {
    serde_json::from_str(&fs::read_to_string(manifest_path(paths, id)).ok()?).ok()
}

/// Returns true when the stored manifest changed.
fn save_manifest(paths: &Paths, id: &str, manifest: &Value) -> bool {
    if load_manifest(paths, id).as_ref() == Some(manifest) {
        return false;
    }
    let path = manifest_path(paths, id);
    crate::util::ensure_private_dir(path.parent().unwrap());
    crate::util::write_if_changed(&path, &manifest.to_string(), Some(0o600), None).is_ok()
}

// ---- one backend process ----

struct Backend {
    stdin: Mutex<ChildStdin>,
    child: Mutex<Child>,
    pending: Mutex<HashMap<u64, Sender<Value>>>,
    next_id: AtomicU64,
    alive: AtomicBool,
    last_used: Mutex<Instant>,
    in_flight: AtomicU64,
    idle: Duration,
}

impl Backend {
    fn spawn(config: &Config, tool: &Tool, paths: &Paths) -> Result<Arc<Backend>, String> {
        let mut cmd = run::command(config, tool, &[])?;
        let log_dir = paths.cache_dir.join("logs");
        crate::util::ensure_private_dir(&log_dir);
        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_dir.join(format!("{}.log", tool.id)))
            .map_err(|e| e.to_string())?;
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::from(log));
        let mut child = cmd.spawn().map_err(|e| format!("{} failed to start: {e}", tool.id))?;
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let backend = Arc::new(Backend {
            stdin: Mutex::new(stdin),
            child: Mutex::new(child),
            pending: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            alive: AtomicBool::new(true),
            last_used: Mutex::new(Instant::now()),
            in_flight: AtomicU64::new(0),
            idle: Duration::from_secs(tool.idle_seconds.unwrap_or(DEFAULT_IDLE)),
        });
        let reader = Arc::clone(&backend);
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
                if msg.get("method").is_some() {
                    // Server-to-client requests (sampling, roots, elicitation)
                    // are not supported through the hub.
                    if let Some(id) = msg.get("id") {
                        let reply = error(id, -32601, "not supported by fusebox hub");
                        let _ = reader.write(&reply);
                    }
                    continue;
                }
                if let Some(id) = msg.get("id").and_then(Value::as_u64)
                    && let Some(tx) = reader.pending.lock().unwrap().remove(&id)
                {
                    let _ = tx.send(msg);
                }
            }
            reader.alive.store(false, Ordering::SeqCst);
            for (_, tx) in reader.pending.lock().unwrap().drain() {
                let _ = tx.send(json!({"error": {"code": -32000, "message": "backend exited"}}));
            }
        });
        Ok(backend)
    }

    fn write(&self, value: &Value) -> Result<(), String> {
        let mut line = value.to_string();
        line.push('\n');
        let mut stdin = self.stdin.lock().unwrap();
        stdin.write_all(line.as_bytes()).and_then(|_| stdin.flush()).map_err(|e| format!("backend write failed: {e}"))
    }

    fn request(&self, method: &str, params: Value, timeout: Option<Duration>) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        self.pending.lock().unwrap().insert(id, tx);
        self.write(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
        let reply = match timeout {
            Some(t) => rx.recv_timeout(t).map_err(|_| format!("{method} timed out"))?,
            None => rx.recv().map_err(|_| "backend exited".to_string())?,
        };
        if let Some(err) = reply.get("error") {
            return Err(err["message"].as_str().unwrap_or("backend error").to_string());
        }
        Ok(reply.get("result").cloned().unwrap_or(Value::Null))
    }

    fn handshake(&self, protocol: &str) -> Result<Value, String> {
        let init = self.request(
            "initialize",
            json!({"protocolVersion": protocol, "capabilities": {},
                   "clientInfo": {"name": "fusebox-hub", "version": env!("CARGO_PKG_VERSION")}}),
            Some(Duration::from_secs(60)),
        )?;
        self.write(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))?;
        Ok(init)
    }

    fn list_tools(&self) -> Result<Vec<Value>, String> {
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let params = cursor.as_ref().map_or(json!({}), |c| json!({"cursor": c}));
            let page = self.request("tools/list", params, Some(Duration::from_secs(60)))?;
            tools.extend(page["tools"].as_array().cloned().unwrap_or_default());
            match page["nextCursor"].as_str() {
                Some(c) if !c.is_empty() => cursor = Some(c.to_string()),
                _ => return Ok(tools),
            }
        }
    }

    fn stop(&self) {
        self.alive.store(false, Ordering::SeqCst);
        let mut child = self.child.lock().unwrap();
        let _ = child.kill();
        let _ = child.wait();
    }

    fn touch(&self) {
        *self.last_used.lock().unwrap() = Instant::now();
    }
}

// ---- hub state ----

struct State {
    paths: Paths,
    config_path: PathBuf,
    app: String,
    config: Mutex<Config>,
    backends: Mutex<HashMap<String, Arc<Backend>>>,
    /// Serialises backend start-up per tool id.
    starting: Mutex<HashMap<String, Gate>>,
    protocol: Mutex<String>,
    out: Out,
    ready: AtomicBool,
}

impl State {
    fn hub_tools(&self) -> Vec<Tool> {
        let config = self.config.lock().unwrap();
        config.tools.iter().filter(|t| t.enabled && t.mode == Mode::Hub && t.apps.get(&self.app)).cloned().collect()
    }

    fn list(&self) -> Vec<Value> {
        let mut out = Vec::new();
        if !self.hub_tools().is_empty() {
            out.extend(meta_tools());
        }
        for tool in self.hub_tools() {
            let Some(manifest) = load_manifest(&self.paths, &tool.id) else { continue };
            let prefix = short_id(&tool);
            for t in manifest["tools"].as_array().into_iter().flatten() {
                let mut t = t.clone();
                let name = t["name"].as_str().unwrap_or("").to_string();
                t["name"] = json!(format!("{prefix}{SEP}{name}"));
                if let Some(desc) = t.get("description").and_then(Value::as_str) {
                    t["description"] = json!(format!("[{}] {desc}", tool.name));
                }
                out.push(t);
            }
        }
        out
    }

    fn instructions(&self) -> String {
        let mut parts = vec![
            "On-demand MCP servers managed by Fusebox. Tools named <server>__<tool> start their server on first use. \
             Servers that never ran have no listed tools yet: call fusebox_servers to see every server (pass `server` \
             to start one and get its tools), then fusebox_call to use a tool."
                .to_string(),
        ];
        for tool in self.hub_tools() {
            if let Some(text) =
                load_manifest(&self.paths, &tool.id).and_then(|m| m["instructions"].as_str().map(String::from))
                && !text.trim().is_empty()
            {
                parts.push(format!("## {}\n{}", tool.name, text.trim()));
            }
        }
        parts.join("\n\n")
    }

    fn notify_list_changed(&self) {
        if self.ready.load(Ordering::SeqCst) {
            send(&self.out, &json!({"jsonrpc": "2.0", "method": "notifications/tools/list_changed"}));
        }
    }

    /// Running backend for `tool`, starting it if needed. Concurrent callers
    /// for the same tool wait for one start instead of spawning twice.
    fn backend(&self, tool: &Tool) -> Result<Arc<Backend>, String> {
        if let Some(b) = self.backends.lock().unwrap().get(&tool.id)
            && b.alive.load(Ordering::SeqCst)
        {
            return Ok(Arc::clone(b));
        }
        let gate = Arc::clone(self.starting.lock().unwrap().entry(tool.id.clone()).or_default());
        let (lock, cvar) = &*gate;
        let mut busy = lock.lock().unwrap();
        while *busy {
            busy = cvar.wait(busy).unwrap();
        }
        if let Some(b) = self.backends.lock().unwrap().get(&tool.id)
            && b.alive.load(Ordering::SeqCst)
        {
            return Ok(Arc::clone(b));
        }
        *busy = true;
        drop(busy);
        let result = self.start(tool);
        *lock.lock().unwrap() = false;
        cvar.notify_all();
        result
    }

    fn start(&self, tool: &Tool) -> Result<Arc<Backend>, String> {
        let config = self.config.lock().unwrap().clone();
        let backend = Backend::spawn(&config, tool, &self.paths)?;
        let protocol = self.protocol.lock().unwrap().clone();
        let init = backend.handshake(&protocol).inspect_err(|_| backend.stop())?;
        let tools = backend.list_tools().inspect_err(|_| backend.stop())?;
        let manifest =
            json!({"tools": tools, "instructions": init["instructions"], "updatedAt": crate::skills::now_iso()});
        self.backends.lock().unwrap().insert(tool.id.clone(), Arc::clone(&backend));
        if save_manifest(&self.paths, &tool.id, &manifest) {
            self.notify_list_changed();
        }
        Ok(backend)
    }

    fn find_server(&self, key: &str) -> Result<Tool, String> {
        self.hub_tools()
            .into_iter()
            .find(|t| short_id(t) == key || t.id == key || t.id.trim_start_matches("agent-") == key)
            .ok_or_else(|| format!("unknown on-demand server: {key}; call fusebox_servers to list them"))
    }

    /// `fusebox_servers`: every on-demand server; with `server`, start it
    /// and return its tools (also caching them for the next tools/list).
    fn servers(&self, args: &Value) -> Result<Value, String> {
        if let Some(key) = args["server"].as_str().filter(|s| !s.is_empty()) {
            let tool = self.find_server(key)?;
            self.backend(&tool)?.touch();
            let manifest = load_manifest(&self.paths, &tool.id).unwrap_or(Value::Null);
            let tools = manifest["tools"].clone();
            let text = json!({"server": short_id(&tool), "name": tool.name, "tools": tools,
                "usage": "call tools with fusebox_call {server, tool, arguments}"});
            return Ok(json!({"content": [{"type": "text", "text": text.to_string()}]}));
        }
        let list: Vec<Value> = self
            .hub_tools()
            .iter()
            .map(|t| {
                let tools: Vec<Value> = load_manifest(&self.paths, &t.id)
                    .and_then(|m| m["tools"].as_array().cloned())
                    .map(|a| a.iter().map(|x| x["name"].clone()).collect())
                    .unwrap_or_default();
                json!({"server": short_id(t), "name": t.name, "description": t.description,
                    "started": !tools.is_empty(), "tools": tools})
            })
            .collect();
        Ok(json!({"content": [{"type": "text", "text": Value::Array(list).to_string()}]}))
    }

    fn call(&self, params: &Value) -> Result<Value, String> {
        let full = params["name"].as_str().ok_or("tools/call requires a name")?;
        let args = &params["arguments"];
        let (tool, name, arguments) = match full {
            "fusebox_servers" => return self.servers(args),
            "fusebox_call" => {
                let server = args["server"].as_str().ok_or("fusebox_call needs `server`")?;
                let name = args["tool"].as_str().ok_or("fusebox_call needs `tool`")?;
                (self.find_server(server)?, name.to_string(), args["arguments"].clone())
            }
            _ => {
                let (prefix, name) = full.split_once(SEP).ok_or_else(|| format!("unknown tool: {full}"))?;
                (self.find_server(prefix)?, name.to_string(), args.clone())
            }
        };
        let backend = self.backend(&tool)?;
        backend.in_flight.fetch_add(1, Ordering::SeqCst);
        backend.touch();
        let forwarded = json!({"name": name, "arguments": if arguments.is_null() { json!({}) } else { arguments }});
        let result = backend.request("tools/call", forwarded, None);
        backend.touch();
        backend.in_flight.fetch_sub(1, Ordering::SeqCst);
        result
    }

    fn reaper(self: &Arc<Self>) {
        let state = Arc::clone(self);
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(5));
                let mut backends = state.backends.lock().unwrap();
                backends.retain(|_, b| {
                    let idle = b.last_used.lock().unwrap().elapsed() >= b.idle;
                    let stop = !b.alive.load(Ordering::SeqCst) || (idle && b.in_flight.load(Ordering::SeqCst) == 0);
                    if stop {
                        b.stop();
                    }
                    !stop
                });
            }
        });
    }

    fn watcher(self: &Arc<Self>) {
        let state = Arc::clone(self);
        std::thread::spawn(move || {
            let stamp = |p: &Path| fs::metadata(p).and_then(|m| m.modified()).ok();
            let mut last: Option<SystemTime> = stamp(&state.config_path);
            loop {
                std::thread::sleep(Duration::from_secs(2));
                let now = stamp(&state.config_path);
                if now == last {
                    continue;
                }
                last = now;
                let Ok(next) = config::load(&state.config_path, &state.paths.secrets_file) else { continue };
                let before: BTreeMap<String, Tool> = state.hub_tools().into_iter().map(|t| (t.id.clone(), t)).collect();
                *state.config.lock().unwrap() = next;
                let after: BTreeMap<String, Tool> = state.hub_tools().into_iter().map(|t| (t.id.clone(), t)).collect();
                if before == after {
                    continue;
                }
                // Restart changed backends lazily; drop removed ones now.
                let mut backends = state.backends.lock().unwrap();
                backends.retain(|id, b| {
                    let keep = after.get(id).is_some_and(|t| before.get(id) == Some(t));
                    if !keep {
                        b.stop();
                    }
                    keep
                });
                drop(backends);
                state.notify_list_changed();
            }
        });
    }
}

/// Number of tools in the cached manifest, if the tool has ever run under
/// the hub or been warmed.
pub fn cached_tool_count(paths: &Paths, id: &str) -> Option<usize> {
    load_manifest(paths, id).and_then(|m| m["tools"].as_array().map(Vec::len))
}

/// Start `tool` once, record its tool manifest and stop it. Returns the
/// number of tools. Used when a tool is switched to hub mode so the very
/// next session already lists its tools.
pub fn warm(paths: &Paths, config: &Config, tool: &Tool) -> crate::util::Result<usize> {
    let backend = Backend::spawn(config, tool, paths)?;
    let result = backend.handshake(DEFAULT_PROTOCOL).and_then(|init| Ok((init, backend.list_tools()?)));
    backend.stop();
    let (init, tools) = result?;
    let count = tools.len();
    let manifest = json!({"tools": tools, "instructions": init["instructions"], "updatedAt": crate::skills::now_iso()});
    save_manifest(paths, &tool.id, &manifest);
    Ok(count)
}

/// Always listed while any on-demand server exists, so agents can reach
/// servers that have never been started (and so have no cached tools).
fn meta_tools() -> Vec<Value> {
    vec![
        json!({
            "name": "fusebox_servers",
            "description": "List the on-demand MCP servers Fusebox can start for you, with their tools. Pass `server` to start one and get its full tool list (names, descriptions, input schemas).",
            "inputSchema": {"type": "object", "properties": {"server": {"type": "string", "description": "server to start, e.g. rea"}}},
        }),
        json!({
            "name": "fusebox_call",
            "description": "Call a tool on an on-demand MCP server (started automatically). Use fusebox_servers first to see the tool names and their input schemas.",
            "inputSchema": {"type": "object", "required": ["server", "tool"], "properties": {
                "server": {"type": "string"}, "tool": {"type": "string"},
                "arguments": {"type": "object", "description": "the tool's arguments"}}},
        }),
    ]
}

pub fn serve(paths: Paths, config_path: PathBuf, app: String) -> crate::util::Result<()> {
    let config = config::load(&config_path, &paths.secrets_file)?;
    let state = Arc::new(State {
        paths,
        config_path,
        app,
        config: Mutex::new(config),
        backends: Mutex::new(HashMap::new()),
        starting: Mutex::new(HashMap::new()),
        protocol: Mutex::new(DEFAULT_PROTOCOL.into()),
        out: Arc::new(Mutex::new(std::io::stdout())),
        ready: AtomicBool::new(false),
    });
    state.reaper();
    state.watcher();
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => {
                send(&state.out, &error(&Value::Null, -32700, "parse error"));
                continue;
            }
        };
        let id = msg.get("id").cloned();
        let method = msg["method"].as_str().unwrap_or("").to_string();
        let Some(id) = id else {
            if method == "notifications/initialized" {
                state.ready.store(true, Ordering::SeqCst);
            }
            continue;
        };
        match method.as_str() {
            "initialize" => {
                let requested = msg["params"]["protocolVersion"].as_str().unwrap_or(DEFAULT_PROTOCOL).to_string();
                *state.protocol.lock().unwrap() = requested.clone();
                let result = json!({
                    "protocolVersion": requested,
                    "capabilities": {"tools": {"listChanged": true}},
                    "serverInfo": {"name": "fusebox-hub", "version": env!("CARGO_PKG_VERSION")},
                    "instructions": state.instructions(),
                });
                send(&state.out, &json!({"jsonrpc": "2.0", "id": id, "result": result}));
            }
            "ping" => send(&state.out, &json!({"jsonrpc": "2.0", "id": id, "result": {}})),
            "tools/list" => {
                send(&state.out, &json!({"jsonrpc": "2.0", "id": id, "result": {"tools": state.list()}}));
            }
            "tools/call" => {
                let state = Arc::clone(&state);
                let params = msg["params"].clone();
                std::thread::spawn(move || {
                    let reply = match state.call(&params) {
                        Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
                        Err(e) => json!({"jsonrpc": "2.0", "id": id, "result": {
                            "content": [{"type": "text", "text": format!("fusebox hub: {e}")}], "isError": true}}),
                    };
                    send(&state.out, &reply);
                });
            }
            _ => send(&state.out, &error(&id, -32601, format!("method not found: {method}"))),
        }
    }
    for b in state.backends.lock().unwrap().values() {
        b.stop();
    }
    Ok(())
}
