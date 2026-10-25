# Agent guide: Agent Switch

Agent Switch is a macOS control plane for local AI agents: one registry of MCP servers, one private credential store, and deterministic per-agent projections for Codex, Claude Code, Claude Desktop, and Hermes. It is a Python 3.11+ CLI (`src/agent_switch`, no runtime dependencies) plus a native SwiftUI app (`macos-app/AgentSwitch`).

## Install

```bash
brew tap risingjerry/tap && brew trust --tap risingjerry/tap && brew install --cask risingjerry/tap/agent-switch
agent-switch --version
agent-switch doctor
```

Contributors, from a checkout (Python 3.11+):

```bash
python3 -m venv .venv && . .venv/bin/activate
python -m pip install -e .
```

`./scripts/install.sh` additionally needs `pipx` and full Xcode; do not run it unless the user asked.

## Build and test

```bash
PYTHONPATH=src python -m unittest discover -s tests
PYTHONPATH=src python -m unittest discover -s tests/integration
PYTHONPATH=src python -m unittest tests.test_docs     # docs only, cheap
python3 scripts/privacy_scan.py                        # credential / private-path scan
zsh -n scripts/install.sh scripts/build-release.sh
swift build --package-path macos-app/AgentSwitch       # app, needs Xcode
```

Tests use isolated temporary homes and never touch real agent configs. Run one full suite at a time.

## Layout

- `src/agent_switch/cli.py`: CLI entry (`agent-switch`); `config/`, `mcp/`, `reconcile/`, `renderers/`, `security/`, `status/` hold registry, wrappers, projections, secret handling and health checks.
- `docs/`: MCP registry, secrets and wrappers, CC Switch compatibility, recovery, roadmap.
- `skills/agent-switch/SKILL.md`: the portable skill for agents that *use* the CLI (not develop it).

## How agents should call the CLI

- Prefer `--json` (`doctor`, `status`, `agents`, `clis`, `skills`, `mcp list`, `secret list`, `reconcile`) and parse it.
- Run `agent-switch doctor` before any MCP or tool change, and `agent-switch reconcile --dry-run` before `agent-switch reconcile`.
- `agent-switch mcp import --dry-run --json` before `--adopt`.
- Generated native entries stay in the `agent-*` namespace; never hand-edit them.

## Never do

- Never print, log, or commit a secret value. `secret list` and diagnostics show names only.
- Never pass a secret as a command argument. Write with `agent-switch secret set --stdin NAME` or `--fd N`; read only with `agent-switch secret get --fd N NAME` into a non-TTY descriptor.
- Never put credentials in project `.env` files, READMEs, native app configs, tests, or fixtures.
- Never run `mcp import --adopt`, `reconcile`, or `secret delete` without the user's explicit go-ahead; they change native configs (with backups).
- Never commit personal paths, real account names, or real tokens. `scripts/privacy_scan.py` must pass.

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full change gate.
