#!/usr/bin/env python3
"""Scenes for docs/demo.gif (synthetic data only). Rebuild: python3 docs/demo/build_demo.py"""
from engine import main

D, OK, CHG, C = "dim", "ok", "chg", "c"

SCENES = [
    {
        "caption": "<b>1</b> &nbsp;Store a credential once, from stdin only",
        "steps": [
            ("cmd", ["pbpaste | agent-switch secret set --stdin SEARCH_API_KEY"], 0.5),
            ("out", [
                ("set SEARCH_API_KEY in ~/.config/agent-switch/secrets.env", C),
                ("# value: ************  (read from stdin: never on argv, never printed)", D),
            ], 2.0),
        ],
    },
    {
        "caption": "<b>2</b> &nbsp;Define the MCP server once, pick the target agents",
        "steps": [
            ("cmd", ["agent-switch mcp add search-docs \\",
                     "  --command npx --arg=-y --arg=@example/search-mcp@1.0.0 \\",
                     "  --secret SEARCH_API_KEY \\",
                     "  --app claude --app claude_desktop --app codex"], 0.5),
            ("out", [("added agent-search-docs", OK)], 1.8),
        ],
    },
    {
        "caption": "<b>3</b> &nbsp;Preview exactly which native configs would change",
        "steps": [
            ("cmd", ["agent-switch reconcile --dry-run"], 0.5),
            ("out", [
                ("Agent Switch status: ok", OK),
                ("Drift changes: 7", CHG),
                ("Missing secrets: none", C),
                ("- change: wrappers: ~/.config/agent-switch/mcp/bin/mcp-search-docs write ...", D),
                ("- change: claude: ~/.claude.json update claude agent-owned MCP projection differs", CHG),
                ("- change: claude_desktop: ~/Library/.../claude_desktop_config.json update ...", CHG),
                ("- change: codex: ~/.codex/config.toml update codex agent-owned MCP projection differs", CHG),
                ("# dry run: nothing written yet", D),
            ], 3.3),
        ],
    },
    {
        "caption": "<b>4</b> &nbsp;Apply once, then check health",
        "steps": [
            ("cmd", ["agent-switch reconcile"], 0.4),
            ("out", [
                ("changed=7 unchanged=0 blocked=0", OK),
                ("Agent Switch status: ok", OK),
                ("Drift changes: 0", C),
                ("Missing secrets: none", C),
            ], 1.4),
            ("cmd", ["agent-switch status"], 0.4),
            ("out", [
                ("Agent Switch status: ok", OK),
                ("Drift changes: 0", C),
                ("Missing secrets: none", C),
                ("# one definition, one credential, every agent in sync", D),
            ], 3.0),
        ],
    },
]

if __name__ == "__main__":
    main(SCENES)
