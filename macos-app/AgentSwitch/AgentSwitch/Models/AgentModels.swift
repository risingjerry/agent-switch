// [INPUT]: Foundation
// [OUTPUT]: Decodable models mirroring the agent-switch CLI JSON output
// [POS]: Models — shared by AgentSwitchService, AppState and all views
// [PROTOCOL]: When this file changes, update this header, then check CLAUDE.md

import Foundation

// MARK: - doctor / status

struct DoctorReport: Decodable {
    let blocked: Bool
    let driftCount: Int
    let findings: [Finding]
    let changes: [PlannedChange]
    let secrets: SecretReport
}

struct Finding: Decodable, Identifiable {
    let severity: String
    let target: String
    let message: String
    var id: String { "\(target)-\(message)" }
}

struct PlannedChange: Decodable, Identifiable {
    let target: String
    let action: String
    let path: String?
    let detail: String
    var id: String { "\(target)-\(action)-\(path ?? "")" }
}

struct SecretReport: Decodable {
    let path: String
    let exists: Bool
    let required: [String]
    let missing: [String]
    let presentNames: [String]
    let storedNames: [String]
    let consumers: [String: [String]]?
}

// MARK: - agents / clis

struct AgentReport: Decodable { let agents: [AgentInfo] }

struct AgentInfo: Decodable, Identifiable {
    let id: String
    let name: String
    let detected: Bool
    let managed: Bool
    let inSync: Bool
    let configPath: String
    let instructionPath: String
}

struct CLIReport: Decodable { let clis: [CLIInfo] }

struct CLIInfo: Decodable, Identifiable {
    let id: String
    let name: String
    let command: String
    let manager: String
    let installed: Bool
    let path: String?
    let version: String?
}

// MARK: - MCP

struct AppFlags: Codable, Equatable {
    var claude: Bool
    var claude_desktop: Bool
    var codex: Bool
    var hermes: Bool
    /// DeepSeek Harness; absent in configs written before it was supported.
    var deepseek: Bool

    static let all = AppFlags(claude: true, claude_desktop: true, codex: true, hermes: true, deepseek: true)

    init(claude: Bool, claude_desktop: Bool, codex: Bool, hermes: Bool, deepseek: Bool) {
        self.claude = claude
        self.claude_desktop = claude_desktop
        self.codex = codex
        self.hermes = hermes
        self.deepseek = deepseek
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        claude = try c.decode(Bool.self, forKey: .claude)
        claude_desktop = try c.decode(Bool.self, forKey: .claude_desktop)
        codex = try c.decode(Bool.self, forKey: .codex)
        hermes = try c.decode(Bool.self, forKey: .hermes)
        deepseek = try c.decodeIfPresent(Bool.self, forKey: .deepseek) ?? false
    }

    var names: [String] {
        [("claude", claude), ("claude_desktop", claude_desktop), ("codex", codex), ("hermes", hermes),
         ("deepseek", deepseek)]
            .filter(\.1).map(\.0)
    }
}

enum MCPMode: String, CaseIterable, Identifiable {
    case off, direct, hub
    var id: String { rawValue }
}

struct MCPList: Decodable { let mcps: [MCPServer] }

struct MCPServer: Decodable, Identifiable, Equatable {
    let id: String
    let name: String
    let command: String
    let args: [String]
    let requiredSecrets: [String]
    let apps: AppFlags
    let envNames: [String]
    let description: String?
    let enabled: Bool
    let mode: String
    let toolCount: Int?

    var effectiveMode: MCPMode { !enabled ? .off : (mode == "hub" ? .hub : .direct) }
    var shortName: String { id.hasPrefix("agent-") ? String(id.dropFirst(6)) : id }
}

/// Full tool entry as stored in config.json (includes non-secret env values).
struct ConfigFile: Decodable {
    struct Tool: Decodable {
        let id: String
        let env: [String: String]?
    }
    let tools: [Tool]
}

struct MCPImportPreview: Decodable {
    struct Skipped: Decodable, Identifiable {
        let app: String
        let id: String
        let reason: String
    }
    let discovered: Int
    let supported: Int
    let skipped: [Skipped]
    let imported: [String]
    let merged: [String]
    let secretNames: [String]
}

// MARK: - Skills

struct SkillReport: Decodable {
    let hubPath: String
    let profiles: [SkillProfile]
    let skills: [SkillInfo]
    let dormantCount: Int
}

struct SkillProfile: Decodable, Identifiable {
    let id: String
    let project: String
    let skillCount: Int
}

struct SkillInfo: Decodable, Identifiable {
    let id: String
    let name: String
    let source: String
    let path: String
    let absolutePath: String
    let profiles: [String]
    let globalActive: Bool
    let status: String
    let exists: Bool
}

struct SkillUsage: Decodable {
    struct Row: Decodable, Identifiable {
        let name: String
        let profiles: [String]
        let claude: Int
        let codex: Int
        let lastUsed: String
        let listingChars: Int
        var id: String { name }
        var total: Int { claude + codex }
    }
    let days: Int
    let sessions: [String: Int]
    let scannedFiles: Int
    let cachedFiles: Int
    let enabled: [Row]
}

struct PrunePlan: Decodable {
    struct Protected: Decodable, Identifiable {
        let name: String
        let reason: String
        var id: String { name }
    }
    let profile: String
    let days: Int
    let applied: Bool
    let disabled: [String]
    let protected: [Protected]
}

struct SecretNames: Decodable {
    let path: String
    let names: [String]
}
