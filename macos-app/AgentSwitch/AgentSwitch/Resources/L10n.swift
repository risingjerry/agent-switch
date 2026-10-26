// [INPUT]: Foundation
// [OUTPUT]: L10n — UI strings in Chinese or English, chosen from the system language
// [POS]: Resources — single source for all user-facing text
// [PROTOCOL]: When this file changes, update this header, then check CLAUDE.md

import Foundation

enum AppVersion {
    static let current = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "0.4.0"
}

enum L10n {
    /// "system" (default), "zh" or "en". The first run follows the macOS
    /// language; the sidebar and Settings switch it live.
    static let languageKey = "language"
    static var chinese: Bool {
        switch UserDefaults.standard.string(forKey: languageKey) {
        case "zh": return true
        case "en": return false
        default: return Locale.preferredLanguages.first?.hasPrefix("zh") ?? false
        }
    }

    static func t(_ en: String, _ zh: String) -> String { chinese ? zh : en }

    static let appName = "Fusebox"

    // Navigation
    static var overview: String { t("Overview", "概览") }
    static let mcp = "MCP"
    static let skills = "Skills"
    static var secrets: String { t("Secrets", "密钥") }
    static var clis: String { t("CLIs", "命令行工具") }
    static var settings: String { t("Settings", "设置") }

    // Common actions
    static var sync: String { t("Sync", "同步") }
    static var refresh: String { t("Refresh", "刷新") }
    static var save: String { t("Save", "保存") }
    static var cancel: String { t("Cancel", "取消") }
    static var delete: String { t("Delete", "删除") }
    static var add: String { t("Add", "添加") }
    static var done: String { t("Done", "完成") }

    // Health
    static var allGood: String { t("Everything is in sync", "一切就绪") }
    static func pending(_ n: Int) -> String { t("\(n) change(s) waiting to sync", "有 \(n) 处改动等待同步") }
    static var blocked: String { t("Sync is blocked — see findings below", "同步被阻止，请看下方问题") }
    static var checking: String { t("Checking…", "正在检查…") }
    static var pendingChanges: String { t("Pending changes", "待同步改动") }
    static var findings: String { t("Findings", "发现的问题") }
    static var agents: String { t("Agents", "Agent") }
    static var notDetected: String { t("Not installed", "未安装") }
    static var synced: String { t("Synced", "已同步") }
    static var needsSync: String { t("Needs sync", "需要同步") }
    static var unmanaged: String { t("Not managed", "未接管") }

    // MCP
    static var modeOff: String { t("Off", "关闭") }
    static var modeDirect: String { t("Always on", "常驻") }
    static var modeHub: String { t("On demand", "按需") }
    static var modeHelp: String { t(
        "Always on: each agent starts it with every session. On demand: it starts on first use and stops when idle.",
        "常驻：每个会话都会启动它。按需：第一次用到才启动，闲置后自动关闭。") }
    static var tools: String { t("tools", "个工具") }
    static var command: String { t("Command", "启动命令") }
    static var arguments: String { t("Arguments", "参数") }
    static var oneArgPerLine: String { t("One argument per line", "每行一个参数") }
    static var requiredSecrets: String { t("Secrets it needs", "需要的密钥") }
    static var environment: String { t("Environment (non-secret)", "环境变量（非密钥）") }
    static var apps: String { t("Use in", "用于") }
    static var description: String { t("Description", "说明") }
    static var newMCP: String { t("New MCP", "新建 MCP") }
    static var importMCPs: String { t("Import from agents…", "从各 Agent 导入…") }
    static var noMCP: String { t("No MCP servers yet", "还没有 MCP") }
    static var selectMCP: String { t("Select an MCP to edit it", "选择一个 MCP 进行编辑") }
    static var missing: String { t("missing", "缺失") }

    // Skills
    static var usage: String { t("Usage", "使用情况") }
    static var library: String { t("Library", "全部 Skill") }
    static var cleanUp: String { t("Clean up…", "清理…") }
    static var lastUsed: String { t("Last used", "最近使用") }
    static var size: String { t("Listing size", "占用字数") }
    static var profiles: String { t("Profiles", "启用于") }
    static var never: String { t("never", "从未") }
    static func unusedSummary(_ unused: Int, _ total: Int, _ days: Int) -> String {
        t("\(unused) of \(total) enabled skills were not used in the last \(days) days",
          "最近 \(days) 天，\(total) 个已启用的 Skill 里有 \(unused) 个没用过")
    }
    static func sessions(_ claude: Int, _ codex: Int, _ deepseek: Int) -> String {
        t("from \(claude) Claude Code, \(codex) Codex and \(deepseek) DeepSeek Harness sessions",
          "统计自 \(claude) 个 Claude Code、\(codex) 个 Codex、\(deepseek) 个 DeepSeek Harness 会话")
    }
    static var suggestions: String { t("Suggestions", "建议") }
    static var askAgent: String { t("Let your agent handle it", "交给 Agent 处理") }
    static var askAgentHint: String {
        t("Fusebox is built for agents to operate. Paste this into Claude Code, Codex or DeepSeek Harness:",
          "这些改动交给 Agent 来做。把下面这句话发给 Claude Code、Codex 或 DeepSeek Harness：")
    }
    static var askAgentPrompt: String {
        t("Check Fusebox (fusebox advise) and tell me what you suggest changing — apply only what I approve.",
          "看看 Fusebox（运行 fusebox advise），告诉我你建议改哪些，我同意的再帮我执行。")
    }
    static var copied: String { t("Copied", "已复制") }
    static var copyPrompt: String { t("Copy", "复制") }
    static var noSuggestions: String { t("Nothing to change — everything is synced and in use.", "没有需要调整的，一切都在正常使用。") }
    static var needsYou: String { t("needs you", "需要你来") }
    static var cleanupTitle: String { t("Turn off unused skills", "停用没用过的 Skill") }
    static var cleanupHint: String { t(
        "Turned-off skills stay on disk and can be turned back on anytime.",
        "停用后文件仍保留在硬盘上，随时可以重新启用。") }
    static var kept: String { t("Kept automatically", "自动保留") }
    static func turnOff(_ n: Int) -> String { t("Turn off \(n)", "停用 \(n) 个") }
    static var nothingToClean: String { t("Every enabled skill was used in this period.", "这段时间里每个启用的 Skill 都用过。") }
    static var profile: String { t("Profile", "范围") }
    static var statusGlobal: String { t("Global", "全局") }
    static var statusProject: String { t("Project", "项目") }
    static var statusDormant: String { t("Off", "未启用") }
    static var statusMissing: String { t("Missing", "缺失") }
    static var search: String { t("Search", "搜索") }

    // Secrets
    static var usedBy: String { t("Used by", "使用者") }
    static var unused: String { t("Not used by any MCP", "没有 MCP 使用") }
    static var newSecret: String { t("New secret", "新建密钥") }
    static var secretName: String { t("Name (e.g. OPENAI_API_KEY)", "名称（例如 OPENAI_API_KEY）") }
    static var secretValue: String { t("Value", "值") }
    static var replace: String { t("Replace…", "替换…") }
    static var reveal: String { t("Reveal for 10 seconds", "显示 10 秒") }
    static var copy: String { t("Copy", "复制") }
    static var secretsFooter: String { t(
        "Values are stored only in the local secret file and are never shown in logs or agent configs.",
        "密钥只保存在本机密钥文件里，不会出现在日志和各 Agent 的配置中。") }
    static var missingSecrets: String { t("Missing", "缺失") }

    // CLIs
    static var installed: String { t("Installed", "已安装") }
    static var notInstalled: String { t("Not installed", "未安装") }
    static var version: String { t("Version", "版本") }
    static var path: String { t("Path", "路径") }

    // Settings
    static var cliPath: String { t("fusebox binary", "fusebox 程序位置") }
    static var cliPathHint: String { t("Leave empty to find it automatically.", "留空则自动查找。") }
    static var usageWindow: String { t("Usage window", "统计时间范围") }
    static func days(_ n: Int) -> String { t("\(n) days", "\(n) 天") }
    static var language: String { t("Language", "语言") }
    static var followSystem: String { t("Follow system", "跟随系统") }
}
