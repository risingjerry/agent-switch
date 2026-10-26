// [INPUT]: Foundation
// [OUTPUT]: L10n — UI strings in Chinese or English, chosen from the system language
// [POS]: Resources — single source for all user-facing text
// [PROTOCOL]: When this file changes, update this header, then check CLAUDE.md

import Foundation

enum AppVersion {
    static let current = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "0.4.0"
}

enum L10n {
    /// "system" (default), "zh" or "en". Read once at launch: the first run
    /// follows the macOS language; Settings can override it.
    static let languageKey = "language"
    static let chinese: Bool = {
        switch UserDefaults.standard.string(forKey: languageKey) {
        case "zh": return true
        case "en": return false
        default: return Locale.preferredLanguages.first?.hasPrefix("zh") ?? false
        }
    }()

    static func t(_ en: String, _ zh: String) -> String { chinese ? zh : en }

    static let appName = "Agent Switch"
    static let aboutAgentSwitch = t("About Agent Switch", "关于 Agent Switch")
    static let quit = t("Quit Agent Switch", "退出 Agent Switch")

    // Navigation
    static let overview = t("Overview", "概览")
    static let mcp = "MCP"
    static let skills = "Skills"
    static let secrets = t("Secrets", "密钥")
    static let clis = t("CLIs", "命令行工具")
    static let settings = t("Settings", "设置")

    // Common actions
    static let sync = t("Sync", "同步")
    static let refresh = t("Refresh", "刷新")
    static let save = t("Save", "保存")
    static let cancel = t("Cancel", "取消")
    static let delete = t("Delete", "删除")
    static let add = t("Add", "添加")
    static let done = t("Done", "完成")

    // Health
    static let allGood = t("Everything is in sync", "一切就绪")
    static func pending(_ n: Int) -> String { t("\(n) change(s) waiting to sync", "有 \(n) 处改动等待同步") }
    static let blocked = t("Sync is blocked — see findings below", "同步被阻止，请看下方问题")
    static let checking = t("Checking…", "正在检查…")
    static let pendingChanges = t("Pending changes", "待同步改动")
    static let findings = t("Findings", "发现的问题")
    static let agents = t("Agents", "Agent")
    static let notDetected = t("Not installed", "未安装")
    static let synced = t("Synced", "已同步")
    static let needsSync = t("Needs sync", "需要同步")
    static let unmanaged = t("Not managed", "未接管")

    // MCP
    static let modeOff = t("Off", "关闭")
    static let modeDirect = t("Always on", "常驻")
    static let modeHub = t("On demand", "按需")
    static let modeHelp = t(
        "Always on: each agent starts it with every session. On demand: it starts on first use and stops when idle.",
        "常驻：每个会话都会启动它。按需：第一次用到才启动，闲置后自动关闭。")
    static let tools = t("tools", "个工具")
    static let command = t("Command", "启动命令")
    static let arguments = t("Arguments", "参数")
    static let oneArgPerLine = t("One argument per line", "每行一个参数")
    static let requiredSecrets = t("Secrets it needs", "需要的密钥")
    static let environment = t("Environment (non-secret)", "环境变量（非密钥）")
    static let apps = t("Use in", "用于")
    static let description = t("Description", "说明")
    static let newMCP = t("New MCP", "新建 MCP")
    static let importMCPs = t("Import from agents…", "从各 Agent 导入…")
    static let noMCP = t("No MCP servers yet", "还没有 MCP")
    static let selectMCP = t("Select an MCP to edit it", "选择一个 MCP 进行编辑")
    static let missing = t("missing", "缺失")

    // Skills
    static let usage = t("Usage", "使用情况")
    static let library = t("Library", "全部 Skill")
    static let cleanUp = t("Clean up…", "清理…")
    static let lastUsed = t("Last used", "最近使用")
    static let size = t("Listing size", "占用字数")
    static let profiles = t("Profiles", "启用于")
    static let never = t("never", "从未")
    static func unusedSummary(_ unused: Int, _ total: Int, _ days: Int) -> String {
        t("\(unused) of \(total) enabled skills were not used in the last \(days) days",
          "最近 \(days) 天，\(total) 个已启用的 Skill 里有 \(unused) 个没用过")
    }
    static func sessions(_ claude: Int, _ codex: Int) -> String {
        t("from \(claude) Claude Code and \(codex) Codex sessions", "统计自 \(claude) 个 Claude Code 会话和 \(codex) 个 Codex 会话")
    }
    static let cleanupTitle = t("Turn off unused skills", "停用没用过的 Skill")
    static let cleanupHint = t(
        "Turned-off skills stay on disk and can be turned back on anytime.",
        "停用后文件仍保留在硬盘上，随时可以重新启用。")
    static let kept = t("Kept automatically", "自动保留")
    static func turnOff(_ n: Int) -> String { t("Turn off \(n)", "停用 \(n) 个") }
    static let nothingToClean = t("Every enabled skill was used in this period.", "这段时间里每个启用的 Skill 都用过。")
    static let profile = t("Profile", "范围")
    static let statusGlobal = t("Global", "全局")
    static let statusProject = t("Project", "项目")
    static let statusDormant = t("Off", "未启用")
    static let statusMissing = t("Missing", "缺失")
    static let search = t("Search", "搜索")

    // Secrets
    static let usedBy = t("Used by", "使用者")
    static let unused = t("Not used by any MCP", "没有 MCP 使用")
    static let newSecret = t("New secret", "新建密钥")
    static let secretName = t("Name (e.g. OPENAI_API_KEY)", "名称（例如 OPENAI_API_KEY）")
    static let secretValue = t("Value", "值")
    static let replace = t("Replace…", "替换…")
    static let reveal = t("Reveal for 10 seconds", "显示 10 秒")
    static let copy = t("Copy", "复制")
    static let secretsFooter = t(
        "Values are stored only in the local secret file and are never shown in logs or agent configs.",
        "密钥只保存在本机密钥文件里，不会出现在日志和各 Agent 的配置中。")
    static let missingSecrets = t("Missing", "缺失")

    // CLIs
    static let installed = t("Installed", "已安装")
    static let notInstalled = t("Not installed", "未安装")
    static let version = t("Version", "版本")
    static let path = t("Path", "路径")

    // Settings
    static let cliPath = t("agent-switch binary", "agent-switch 程序位置")
    static let cliPathHint = t("Leave empty to find it automatically.", "留空则自动查找。")
    static let usageWindow = t("Usage window", "统计时间范围")
    static func days(_ n: Int) -> String { t("\(n) days", "\(n) 天") }
    static let language = t("Language", "语言")
    static let followSystem = t("Follow system", "跟随系统")
    static let restartHint = t("Takes effect after reopening the app.", "重新打开 App 后生效。")
}
