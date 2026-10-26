//! End-to-end test of `agent-switch hub` against a fake stdio MCP server.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

struct Hub {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<Value>,
}

impl Hub {
    fn send(&mut self, v: Value) {
        writeln!(self.stdin, "{v}").unwrap();
        self.stdin.flush().unwrap();
    }

    /// Next message matching `pred`, skipping others.
    fn wait(&self, what: &str, pred: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let msg = self.rx.recv_timeout(left).unwrap_or_else(|_| panic!("timed out waiting for {what}"));
            if pred(&msg) {
                return msg;
            }
        }
    }

    fn response(&self, id: i64) -> Value {
        self.wait(&format!("response {id}"), |m| m["id"] == json!(id))
    }
}

impl Drop for Hub {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

fn write_config(home: &Path, fixture: &Path, enabled: bool) {
    let config = json!({
        "secretFile": home.join("secrets.env").to_string_lossy(),
        "routes": {},
        "tools": [{
            "id": "agent-fake", "name": "fake", "command": "python3",
            "args": [fixture.to_string_lossy()], "requiredSecrets": ["FAKE_TOKEN"],
            "apps": {"claude": true, "claude_desktop": false, "codex": false, "hermes": false},
            "env": {"FAKE_LOG": home.join("fake.log").to_string_lossy()},
            "enabled": enabled, "mode": "hub", "idleSeconds": 1, "description": null,
        }],
    });
    fs::write(home.join("config.json"), config.to_string()).unwrap();
}

fn starts(home: &Path) -> Vec<u32> {
    fs::read_to_string(home.join("fake.log"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.strip_prefix("start ")?.parse().ok())
        .collect()
}

fn alive(pid: u32) -> bool {
    Command::new("kill").args(["-0", &pid.to_string()]).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

#[test]
fn hub_is_lazy_scoped_reaps_and_reloads() {
    let root: PathBuf = std::env::temp_dir().join(format!("as-hub-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let home = root.join("agent");
    fs::create_dir_all(&home).unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_mcp.py");
    fs::write(home.join("secrets.env"), "FAKE_TOKEN=tok-1\nOTHER_SECRET=nope\n").unwrap();
    write_config(&home, &fixture, true);

    let mut child = Command::new(env!("CARGO_BIN_EXE_agent-switch"))
        .args(["--home", &home.to_string_lossy(), "--user-home", &root.to_string_lossy(), "hub", "claude"])
        .env("AGENT_SWITCH_CACHE", root.join("cache"))
        .env("OTHER_API_KEY", "must-not-leak")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                let _ = tx.send(v);
            }
        }
    });
    let mut hub = Hub { child, stdin, rx };

    hub.send(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}}));
    let init = hub.response(1);
    assert_eq!(init["result"]["capabilities"]["tools"]["listChanged"], json!(true));
    assert!(starts(&home).is_empty(), "initialize must not start backends");

    // First run: no manifest yet, so the hub builds one in the background
    // and announces the new tools.
    hub.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
    hub.wait("list_changed after warm-up", |m| m["method"] == "notifications/tools/list_changed");
    hub.send(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
    let list = hub.response(2);
    let names: Vec<&str> =
        list["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, vec!["fake__echo"]);

    // Call: backend runs with its declared secret only.
    hub.send(json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
        "params": {"name": "fake__echo", "arguments": {"text": "hi"}}}));
    let call = hub.response(3);
    assert_eq!(call["result"]["content"][0]["text"], json!("echo:hi token=tok-1 leaked=[]"));

    // Idle reaper stops the backend (idleSeconds = 1, reaper ticks every 5s).
    let pid = *starts(&home).last().unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while alive(pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(!alive(pid), "idle backend should have been stopped");

    // A later call starts it again transparently.
    hub.send(json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call",
        "params": {"name": "fake__echo", "arguments": {"text": "again"}}}));
    assert_eq!(hub.response(4)["result"]["content"][0]["text"], json!("echo:again token=tok-1 leaked=[]"));

    // Hot reload: disabling the tool in config removes it live.
    std::thread::sleep(Duration::from_millis(1100));
    write_config(&home, &fixture, false);
    hub.wait("list_changed after config edit", |m| m["method"] == "notifications/tools/list_changed");
    hub.send(json!({"jsonrpc": "2.0", "id": 5, "method": "tools/list"}));
    assert_eq!(hub.response(5)["result"]["tools"], json!([]));
    hub.send(
        json!({"jsonrpc": "2.0", "id": 6, "method": "tools/call", "params": {"name": "fake__echo", "arguments": {}}}),
    );
    assert_eq!(hub.response(6)["result"]["isError"], json!(true));

    drop(hub);
    let _ = fs::remove_dir_all(&root);
}
