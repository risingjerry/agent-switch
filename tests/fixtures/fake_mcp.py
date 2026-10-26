"""Minimal stdio MCP server for hub tests. Appends its pid to $FAKE_LOG on start."""
import json, os, sys

with open(os.environ["FAKE_LOG"], "a") as log:
    log.write(f"start {os.getpid()}\n")

def reply(msg_id, result):
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": msg_id, "result": result}) + "\n")
    sys.stdout.flush()

for line in sys.stdin:
    msg = json.loads(line)
    method, msg_id = msg.get("method"), msg.get("id")
    if method == "initialize":
        reply(msg_id, {"protocolVersion": msg["params"]["protocolVersion"], "capabilities": {"tools": {}},
                       "serverInfo": {"name": "fake", "version": "1"}, "instructions": "fake instructions"})
    elif method == "tools/list":
        extra = [{"name": f"tool_{i}", "description": f"Extra {i}\nmore", "inputSchema": {"type": "object"}}
                 for i in range(int(os.environ.get("FAKE_EXTRA", "0")))]
        reply(msg_id, {"tools": [{"name": "echo", "description": "Echo back", "inputSchema": {"type": "object"}}] + extra})
    elif method == "tools/call":
        leaked = sorted(k for k in os.environ if k.startswith("OTHER_"))
        text = f"echo:{msg['params']['arguments'].get('text')} token={os.environ.get('FAKE_TOKEN')} leaked={leaked}"
        reply(msg_id, {"content": [{"type": "text", "text": text}]})
