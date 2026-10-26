#!/bin/zsh
# A throwaway HOME with synthetic Skills and MCP servers, for screenshots and
# the README demo. Usage: docs/demo/make-demo-home.sh DIR
set -euo pipefail
ROOT="${0:A:h:h:h}"
H="${1:?usage: make-demo-home.sh DIR}"
mkdir -p "$H/.claude" "$H/.codex" "$H/skills"
skill() {
  mkdir -p "$H/skills/$1"
  printf -- '---\nname: %s\ndescription: %s\n---\n\n# %s\n' "$1" "$2" "$1" > "$H/skills/$1/SKILL.md"
}
skill pdf-tools "Merge, split, rotate and extract text from PDF files."
skill release-notes "Draft release notes and a changelog entry from merged pull requests."
skill deploy-preview "Deploy a branch to a preview environment and post the URL on the PR."
skill db-migrations "Write, check and roll back database schema migrations safely."
skill invoice-ocr "Read invoices and receipts (PDF or photo) into structured rows."
skill seo-audit "Audit a web page for SEO and answer-engine visibility; suggest fixes."
skill slide-deck "Turn an outline into a clean slide deck with speaker notes."
skill weekly-report "Summarise the week's commits, issues and calendar into a status report."
F() { HOME="$H" "$ROOT/target/release/fusebox" "$@"; }
F skills add-source team --path "$H/skills" >/dev/null
F mcp add browser --name "Browser" --command python3 --arg "$ROOT/tests/fixtures/fake_mcp.py" \
  --description "Open pages, click, fill forms and take screenshots" --env FAKE_LOG=/dev/null >/dev/null
F mcp add github --name "GitHub" --command python3 --arg "$ROOT/tests/fixtures/fake_mcp.py" \
  --secret GITHUB_TOKEN --description "Issues, pull requests and code search" --env FAKE_LOG=/dev/null >/dev/null
printf 'demo-token-not-real' | F secret set --stdin GITHUB_TOKEN >/dev/null
F reconcile >/dev/null
print "demo HOME ready: $H"
