from __future__ import annotations

import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


class DocsTests(unittest.TestCase):
    def test_docs_reference_current_commands_and_boundaries(self) -> None:
        readme = (ROOT / "README.md").read_text()
        readme_zh = (ROOT / "README.zh-CN.md").read_text()
        compat = (ROOT / "docs" / "ccswitch-compat.md").read_text()
        recovery = (ROOT / "docs" / "recovery.md").read_text()
        secrets = (ROOT / "docs" / "secrets-and-wrappers.md").read_text()
        registry = (ROOT / "docs" / "mcp-registry.md").read_text()
        roadmap = (ROOT / "docs" / "roadmap.md").read_text()
        instructions = (ROOT / "src" / "agent_switch" / "instructions.py").read_text()
        self.assertIn("agent-switch doctor", readme)
        self.assertIn("README.zh-CN.md", "\n".join(readme.splitlines()[:12]))
        self.assertIn("README.md", "\n".join(readme_zh.splitlines()[:12]))
        install_command = (
            "brew tap risingjerry/tap && brew trust --tap risingjerry/tap && "
            "brew install --cask risingjerry/tap/agent-switch"
        )
        self.assertIn(install_command, readme)
        self.assertIn(install_command, readme_zh)
        source_command = (
            "git clone https://github.com/risingjerry/agent-switch.git && "
            "cd agent-switch && ./scripts/install.sh"
        )
        self.assertIn(source_command, readme)
        self.assertIn(source_command, readme_zh)
        self.assertIn("removes Gatekeeper quarantine", readme)
        self.assertIn("移除 Gatekeeper quarantine", readme_zh)
        self.assertIn("agent-*", compat)
        self.assertIn("CC Switch", compat)
        self.assertIn("agent-switch reconcile", recovery)
        self.assertIn("secret set --stdin", secrets)
        self.assertIn("secret set --fd", secrets)
        self.assertIn("secret get --fd", secrets)
        self.assertIn("secret delete", secrets)
        self.assertIn("without a system authentication dialog", secrets)
        self.assertNotIn("device-owner authentication", secrets)
        self.assertIn("agent-switch clis", readme)
        self.assertIn("agent-switch skills", readme)
        self.assertIn("agent-switch mcp import --adopt", readme)
        self.assertIn("mcp add|set|enable|disable|remove|list", (ROOT / "llms.txt").read_text())
        self.assertIn("injects only those declared values", secrets)
        self.assertIn("Neutral first run", registry)
        self.assertIn("Streamable HTTP", roadmap)
        self.assertLess(
            readme.index("## Quick start"),
            readme.index("## What Agent Switch manages"),
        )
        self.assertIn("agent-switch agents --json", recovery)
        self.assertIn("secret set --stdin", instructions)
        self.assertIn("secret get --fd", instructions)
        self.assertNotIn("secret set NAME VALUE` to add", instructions)


if __name__ == "__main__":
    unittest.main()
