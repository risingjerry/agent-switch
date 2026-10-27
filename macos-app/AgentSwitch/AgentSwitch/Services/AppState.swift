// [INPUT]: SwiftUI, Observation, AgentSwitchService, AgentModels
// [OUTPUT]: AppState — observable root state and every user action
// [POS]: Services — injected into all views through the environment
// [PROTOCOL]: When this file changes, update this header, then check CLAUDE.md

import Observation
import SwiftUI

@MainActor
@Observable
final class AppState {
    let service = AgentSwitchService()

    var doctor: DoctorReport?
    var agents: [AgentInfo] = []
    var mcps: [MCPServer] = []
    var skills: SkillReport?
    var usage: SkillUsage?
    var advice: Advice?
    var secretNames: [String] = []
    var clis: [CLIInfo] = []

    var page: Page = .overview
    var busy = false
    var syncing = false
    var loadingUsage = false
    var error: String?

    var usageDays: Int = UserDefaults.standard.object(forKey: "usageDays") as? Int ?? 60 {
        didSet { UserDefaults.standard.set(usageDays, forKey: "usageDays") }
    }

    var pendingCount: Int { doctor?.driftCount ?? 0 }
    var needsSync: Bool { pendingCount > 0 }

    // MARK: Loading

    func refresh() async {
        busy = true
        defer { busy = false }
        async let doctor = attempt { try await self.service.doctor() }
        async let agents = attempt { try await self.service.agents() }
        async let mcps = attempt { try await self.service.mcps() }
        async let skills = attempt { try await self.service.skills() }
        async let secrets = attempt { try await self.service.secretNames() }
        self.doctor = await doctor ?? self.doctor
        // Drift from outside Fusebox (a Skill repo pulled, files edited by
        // hand) is just a stale projection: sync it instead of asking.
        if let d = self.doctor, d.driftCount > 0, !d.blocked,
           (try? await service.reconcile()) != nil {
            self.doctor = await attempt { try await self.service.doctor() } ?? self.doctor
        }
        self.agents = await agents ?? self.agents
        self.mcps = await mcps ?? self.mcps
        self.skills = await skills ?? self.skills
        self.secretNames = await secrets ?? self.secretNames
        if advice != nil { await loadAdvice() }
    }

    func loadUsage() async {
        loadingUsage = true
        defer { loadingUsage = false }
        usage = await attempt { try await self.service.usage(days: self.usageDays) } ?? usage
    }

    func loadAdvice() async {
        advice = await attempt { try await self.service.advise(days: self.usageDays) } ?? advice
    }

    func loadCLIs() async {
        clis = await attempt { try await self.service.clis() } ?? clis
    }

    // MARK: Actions

    /// Every write is followed by a reconcile so agents pick it up; the
    /// result is then re-read so the UI never shows stale state.
    func perform(_ action: @escaping () async throws -> Void, reconcile: Bool = true) async {
        syncing = true
        defer { syncing = false }
        do {
            try await action()
            if reconcile { try await service.reconcile() }
        } catch {
            self.error = error.localizedDescription
        }
        await refresh()
    }

    func sync() async { await perform({}) }

    func setMode(_ server: MCPServer, _ mode: MCPMode) async {
        await perform { try await self.service.setMode(id: server.id, mode: mode) }
    }

    func save(_ draft: MCPDraft) async { await perform { try await self.service.saveMCP(draft) } }
    func remove(_ server: MCPServer) async { await perform { try await self.service.removeMCP(id: server.id) } }
    func importMCPs() async { await perform { try await self.service.importMCPs() } }

    func disableSkills(_ names: [String], profile: String) async {
        await perform({ try await self.service.disableSkills(names, profile: profile) }, reconcile: false)
        await loadUsage()
    }

    func setSecret(_ name: String, _ value: String) async {
        await perform({ try await self.service.setSecret(name: name, value: value) }, reconcile: false)
    }

    func deleteSecret(_ name: String) async {
        await perform({ try await self.service.deleteSecret(name: name) }, reconcile: false)
    }

    private func attempt<T>(_ body: @escaping () async throws -> T) async -> T? {
        do {
            return try await body()
        } catch {
            self.error = error.localizedDescription
            return nil
        }
    }

    /// MCP ids that need each secret name.
    func consumers(of name: String) -> [String] {
        mcps.filter { $0.enabled && $0.requiredSecrets.contains(name) }.map(\.shortName)
    }

    var missingSecrets: [String] { doctor?.secrets.missing ?? [] }
}
