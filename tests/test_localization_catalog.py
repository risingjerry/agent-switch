import json
import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
APP = ROOT / "macos-app" / "AgentSwitch"
L10N = APP / "AgentSwitch" / "Resources" / "L10n.swift"
CATALOG = APP / "AgentSwitch" / "Resources" / "Localizable.xcstrings"


class LocalizationCatalogTests(unittest.TestCase):
    def test_every_declared_key_has_complete_english_and_chinese_text(self) -> None:
        declared = set(re.findall(r'localized\("([^"]+)"\)', L10N.read_text()))
        strings = json.loads(CATALOG.read_text())["strings"]

        self.assertEqual(declared, set(strings), "L10n.swift and the string catalog must stay in sync")
        self.assertGreaterEqual(len(strings), 165)

        for key, entry in strings.items():
            localizations = entry.get("localizations", {})
            for language in ("en", "zh-Hans"):
                value = localizations.get(language, {}).get("stringUnit", {}).get("value", "")
                self.assertTrue(value.strip(), f"{key} is missing a {language} translation")

    def test_build_systems_package_the_string_catalog(self) -> None:
        package = (APP / "Package.swift").read_text()
        project = (APP / "AgentSwitch.xcodeproj" / "project.pbxproj").read_text()

        self.assertIn('Resources/Localizable.xcstrings', package)
        self.assertIn('Localizable.xcstrings', project)
        self.assertIn('zh-Hans', project)

    def test_readmes_use_the_sanitized_english_dashboard(self) -> None:
        image = "docs/assets/agent-switch-dashboard-en.png"
        self.assertTrue((ROOT / image).is_file())
        for readme in ("README.md", "README.zh-CN.md"):
            self.assertIn(image, (ROOT / readme).read_text())

    def test_readmes_embed_the_privacy_safe_repository_metrics_chart(self) -> None:
        chart = "https://raw.githubusercontent.com/risingjerry/agent-switch/metrics/repository-metrics.svg"
        for readme in ("README.md", "README.zh-CN.md"):
            content = (ROOT / readme).read_text()
            self.assertIn(chart, content)
            self.assertIn("api.star-history.com/svg?repos=risingjerry/agent-switch", content)

    def test_activity_chart_stays_near_the_end_after_product_documentation(self) -> None:
        english = (ROOT / "README.md").read_text()
        self.assertLess(english.index("## Development"), english.index("## Project activity"))
        self.assertLess(english.index("## Project activity"), english.index("## License"))

        chinese = (ROOT / "README.zh-CN.md").read_text()
        self.assertLess(chinese.index("## 开发"), chinese.index("## 项目数据"))
        self.assertLess(chinese.index("## 项目数据"), chinese.index("## 许可证"))


if __name__ == "__main__":
    unittest.main()
