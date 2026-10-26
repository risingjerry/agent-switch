// [INPUT]: Foundation, Darwin, AgentModels
// [OUTPUT]: AgentSwitchService — async bridge to the agent-switch binary
// [POS]: Services — the only place that launches processes
// [PROTOCOL]: When this file changes, update this header, then check CLAUDE.md

import Darwin
import Foundation

actor AgentSwitchService {
    static let cliPathKey = "cliPath"

    private func binary() throws -> String {
        let home = NSHomeDirectory()
        let custom = UserDefaults.standard.string(forKey: Self.cliPathKey).flatMap { $0.isEmpty ? nil : $0 }
        let candidates = [
            custom,
            ProcessInfo.processInfo.environment["AGENT_SWITCH_CLI"],
            home + "/.local/bin/agent-switch",
            "/opt/homebrew/bin/agent-switch",
            "/usr/local/bin/agent-switch",
            home + "/.cargo/bin/agent-switch",
        ].compactMap { $0 }
        guard let path = candidates.first(where: { FileManager.default.isExecutableFile(atPath: $0) }) else {
            throw ServiceError.binaryNotFound
        }
        return path
    }

    // MARK: Reads

    func doctor() async throws -> DoctorReport { try await decode(["doctor", "--json"]) }
    func agents() async throws -> [AgentInfo] { try await decode(["agents", "--json"], as: AgentReport.self).agents }
    func clis() async throws -> [CLIInfo] { try await decode(["clis", "--json"], as: CLIReport.self).clis }
    func mcps() async throws -> [MCPServer] { try await decode(["mcp", "list", "--json"], as: MCPList.self).mcps }
    func skills() async throws -> SkillReport { try await decode(["skills", "--json"]) }
    func secretNames() async throws -> [String] { try await decode(["secret", "list", "--json"], as: SecretNames.self).names }

    func usage(days: Int) async throws -> SkillUsage {
        try await decode(["skills", "usage", "--days", String(days), "--json"])
    }

    func prunePlan(profile: String, days: Int) async throws -> PrunePlan {
        try await decode(["skills", "prune", "--profile", profile, "--days", String(days), "--json"])
    }

    func importPreview() async throws -> MCPImportPreview {
        try await decode(["mcp", "import", "--dry-run", "--json"])
    }

    /// Non-secret env values are only in config.json, not in `mcp list`.
    func envValues(id: String) -> [String: String] {
        let path = NSHomeDirectory() + "/.config/agent-switch/config.json"
        guard let data = FileManager.default.contents(atPath: path),
              let config = try? JSONDecoder().decode(ConfigFile.self, from: data)
        else { return [:] }
        return config.tools.first { $0.id == id }?.env ?? [:]
    }

    // MARK: Writes

    func reconcile() async throws { _ = try await run(["reconcile", "--json"]) }

    func setMode(id: String, mode: MCPMode) async throws {
        switch mode {
        case .off:
            _ = try await run(["mcp", "disable", id, "--json"])
        case .direct:
            _ = try await run(["mcp", "direct", id, "--json"])
            _ = try await run(["mcp", "enable", id, "--json"])
        case .hub:
            _ = try await run(["mcp", "enable", id, "--json"])
            _ = try await run(["mcp", "hub", id, "--json"])
        }
    }

    func saveMCP(_ draft: MCPDraft) async throws {
        var args = ["mcp", "set", draft.id, "--name", draft.name, "--command", draft.command]
        args += draft.args.map { "--arg=\($0)" }
        args += draft.secrets.map { "--secret=\($0)" }
        args += draft.env.sorted { $0.key < $1.key }.map { "--env=\($0.key)=\($0.value)" }
        args += draft.apps.names.flatMap { ["--app", $0] }
        if !draft.description.isEmpty { args += ["--description", draft.description] }
        if !draft.enabled { args.append("--disabled") }
        _ = try await run(args + ["--json"])
    }

    func removeMCP(id: String) async throws { _ = try await run(["mcp", "remove", id, "--json"]) }
    func importMCPs() async throws { _ = try await run(["mcp", "import", "--adopt", "--json"]) }

    func disableSkills(_ names: [String], profile: String) async throws {
        for name in names {
            _ = try await run(["skills", "disable", profile, name])
        }
        _ = try await run(profile == "global" ? ["skills", "sync-global", "--prune"] : ["skills", "sync", profile, "--prune"])
    }

    func setSecret(name: String, value: String) async throws {
        _ = try await run(["secret", "set", "--stdin", name], stdin: Data(value.utf8))
    }

    func deleteSecret(name: String) async throws { _ = try await run(["secret", "delete", name]) }

    /// Reads a secret through a private FIFO bound to fd 3 of the CLI; the
    /// value never passes through stdout, a regular file or arguments.
    func revealSecret(name: String) async throws -> String {
        let executable = try binary()
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("agent-switch-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: false,
                                                attributes: [.posixPermissions: 0o700])
        defer { try? FileManager.default.removeItem(at: dir) }
        let fifo = dir.appendingPathComponent("value")
        guard mkfifo(fifo.path, S_IRUSR | S_IWUSR) == 0 else { throw ServiceError.invalidSecretResponse }
        let reader = Task.detached(priority: .userInitiated) { () throws -> Data in
            let handle = try FileHandle(forReadingFrom: fifo)
            defer { try? handle.close() }
            return try handle.readToEnd() ?? Data()
        }
        do {
            _ = try await runProcess("/bin/sh", ["-c", "exec \"$0\" secret get --fd 3 \"$1\" 3>\"$2\"", executable, name, fifo.path])
        } catch {
            reader.cancel()
            // Unblock the reader if the writer never opened the FIFO.
            if let h = FileHandle(forWritingAtPath: fifo.path) { try? h.close() }
            throw error
        }
        let data = try await reader.value
        guard let value = String(data: data, encoding: .utf8), !value.isEmpty else { throw ServiceError.invalidSecretResponse }
        return value
    }

    // MARK: Process plumbing

    private func decode<T: Decodable>(_ args: [String], as: T.Type = T.self) async throws -> T {
        let output = try await run(args)
        do {
            return try JSONDecoder().decode(T.self, from: Data(output.utf8))
        } catch {
            throw ServiceError.badOutput(args.prefix(2).joined(separator: " "))
        }
    }

    private func run(_ args: [String], stdin: Data? = nil) async throws -> String {
        try await runProcess(try binary(), args, stdin: stdin)
    }

    private func runProcess(_ executable: String, _ args: [String], stdin: Data? = nil) async throws -> String {
        try await withCheckedThrowingContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async {
                let process = Process()
                let out = Pipe()
                let err = Pipe()
                process.executableURL = URL(fileURLWithPath: executable)
                process.arguments = args
                process.standardOutput = out
                process.standardError = err
                let input = stdin.map { _ in Pipe() }
                process.standardInput = input ?? FileHandle.nullDevice
                var env = ProcessInfo.processInfo.environment
                let home = NSHomeDirectory()
                env["PATH"] = [home + "/.local/bin", home + "/.cargo/bin", "/opt/homebrew/bin", "/usr/local/bin",
                               "/usr/bin", "/bin", "/usr/sbin", "/sbin"].joined(separator: ":")
                process.environment = env
                do {
                    try process.run()
                    if let stdin, let input {
                        input.fileHandleForWriting.write(stdin)
                        try input.fileHandleForWriting.close()
                    }
                    // Drain stderr concurrently so a chatty command cannot block.
                    var errData = Data()
                    let group = DispatchGroup()
                    group.enter()
                    DispatchQueue.global().async {
                        errData = err.fileHandleForReading.readDataToEndOfFile()
                        group.leave()
                    }
                    let outData = out.fileHandleForReading.readDataToEndOfFile()
                    group.wait()
                    process.waitUntilExit()
                    let output = String(data: outData, encoding: .utf8) ?? ""
                    guard process.terminationStatus == 0 else {
                        let detail = String(data: errData, encoding: .utf8) ?? output
                        throw ServiceError.cli(detail.trimmingCharacters(in: .whitespacesAndNewlines))
                    }
                    continuation.resume(returning: output)
                } catch {
                    continuation.resume(throwing: error)
                }
            }
        }
    }
}

/// Editable copy of an MCP entry.
struct MCPDraft: Equatable {
    var id: String
    var name: String
    var command: String
    var args: [String]
    var secrets: [String]
    var env: [String: String]
    var apps: AppFlags
    var description: String
    var enabled: Bool

    static func new() -> MCPDraft {
        MCPDraft(id: "", name: "", command: "", args: [], secrets: [], env: [:], apps: .all, description: "", enabled: true)
    }

    init(id: String, name: String, command: String, args: [String], secrets: [String], env: [String: String],
         apps: AppFlags, description: String, enabled: Bool) {
        self.id = id; self.name = name; self.command = command; self.args = args; self.secrets = secrets
        self.env = env; self.apps = apps; self.description = description; self.enabled = enabled
    }

    init(_ server: MCPServer, env: [String: String]) {
        self.init(id: server.id, name: server.name, command: server.command, args: server.args,
                  secrets: server.requiredSecrets, env: env, apps: server.apps,
                  description: server.description ?? "", enabled: server.enabled)
    }
}

enum ServiceError: LocalizedError {
    case binaryNotFound
    case cli(String)
    case badOutput(String)
    case invalidSecretResponse

    var errorDescription: String? {
        switch self {
        case .binaryNotFound:
            return L10n.t("agent-switch was not found. Install it or set its path in Settings.",
                          "找不到 agent-switch 程序，请先安装，或在设置里填写它的位置。")
        case .cli(let detail): return detail.replacingOccurrences(of: "agent-switch: ", with: "")
        case .badOutput(let command): return L10n.t("Unexpected output from `\(command)`", "`\(command)` 的输出无法识别")
        case .invalidSecretResponse: return L10n.t("Could not read the secret", "读取密钥失败")
        }
    }
}
