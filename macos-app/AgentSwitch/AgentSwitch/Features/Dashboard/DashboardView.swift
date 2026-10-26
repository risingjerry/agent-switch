// [INPUT]: SwiftUI, AppState, DS components, AgentsStrip
// [OUTPUT]: DashboardView — the Overview page
// [POS]: Features/Dashboard — first page; links into the other pages
// [PROTOCOL]: When this file changes, update this header, then check CLAUDE.md

import SwiftUI

struct DashboardView: View {
    @Environment(AppState.self) private var state
    @Binding var page: Page

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: DS.page) {
                hero
                tiles
                AgentsStrip(agents: state.agents)
                if let doctor = state.doctor, !doctor.findings.isEmpty {
                    findings(doctor.findings)
                }
                if let doctor = state.doctor, !doctor.changes.isEmpty {
                    changes(doctor.changes)
                }
            }
            .padding(DS.page)
            .frame(maxWidth: 1000, alignment: .leading)
        }
        .navigationTitle(L10n.overview)
        .task { if state.usage == nil { await state.loadUsage() } }
    }

    private var hero: some View {
        HStack(spacing: DS.pad) {
            Image(systemName: heroSymbol)
                .font(.system(size: 34, weight: .medium))
                .foregroundStyle(heroColor)
                .symbolEffect(.pulse, isActive: state.syncing)
            VStack(alignment: .leading, spacing: 4) {
                Text(heroTitle).font(.title.weight(.semibold))
                Text(L10n.t("Secrets, MCP servers and Skills for Claude Code, Codex and Hermes — managed in one place.",
                            "Claude Code、Codex、Hermes 的密钥、MCP 和 Skill，统一在这里管理。"))
                    .foregroundStyle(.secondary)
            }
            Spacer()
            if state.needsSync {
                Button {
                    Task { await state.sync() }
                } label: {
                    Label(L10n.sync, systemImage: "arrow.triangle.2.circlepath")
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.large)
                .disabled(state.syncing)
            }
        }
    }

    private var heroTitle: String {
        guard let doctor = state.doctor else { return L10n.checking }
        if doctor.blocked { return L10n.blocked }
        return state.needsSync ? L10n.pending(state.pendingCount) : L10n.allGood
    }

    private var heroSymbol: String {
        guard let doctor = state.doctor else { return "circle.dotted" }
        if doctor.blocked { return "exclamationmark.octagon.fill" }
        return state.needsSync ? "arrow.triangle.2.circlepath.circle.fill" : "checkmark.seal.fill"
    }

    private var heroColor: Color {
        guard let doctor = state.doctor else { return DS.quiet }
        if doctor.blocked { return DS.bad }
        return state.needsSync ? DS.warn : DS.good
    }

    private var tiles: some View {
        let enabled = state.mcps.filter(\.enabled)
        let hub = enabled.filter { $0.mode == "hub" }.count
        let usage = state.usage
        let unusedSkills = usage?.enabled.filter { $0.total == 0 }.count
        let missing = state.missingSecrets.count
        return LazyVGrid(columns: [GridItem(.adaptive(minimum: 210), spacing: DS.pad)], spacing: DS.pad) {
            tile(.mcp) {
                StatTile(symbol: Page.mcp.symbol, title: L10n.mcp, value: "\(enabled.count)",
                         detail: L10n.t("\(hub) on demand · \(state.mcps.count - enabled.count) off",
                                        "\(hub) 个按需 · \(state.mcps.count - enabled.count) 个关闭"))
            }
            tile(.skills) {
                StatTile(symbol: Page.skills.symbol, title: L10n.skills, value: "\(usage?.enabled.count ?? 0)",
                         detail: unusedSkills.map {
                             L10n.t("\($0) unused in \(state.usageDays) days", "\($0) 个 \(state.usageDays) 天没用过")
                         } ?? L10n.checking,
                         tint: (unusedSkills ?? 0) > 0 ? DS.warn : .accentColor)
            }
            tile(.secrets) {
                StatTile(symbol: Page.secrets.symbol, title: L10n.secrets, value: "\(state.secretNames.count)",
                         detail: missing > 0 ? L10n.t("\(missing) missing", "缺 \(missing) 个")
                                             : L10n.t("Nothing missing", "没有缺失"),
                         tint: missing > 0 ? DS.bad : .accentColor)
            }
            tile(.overview) {
                let synced = state.agents.filter(\.inSync).count
                let detected = state.agents.filter(\.detected).count
                StatTile(symbol: "person.2.wave.2", title: L10n.agents, value: "\(synced)/\(detected)",
                         detail: L10n.t("agents in sync", "个 Agent 已同步"),
                         tint: synced == detected ? DS.good : DS.warn)
            }
        }
    }

    private func tile(_ target: Page, @ViewBuilder _ content: () -> some View) -> some View {
        Button { page = target } label: { content() }
            .buttonStyle(.plain)
    }

    private func findings(_ items: [Finding]) -> some View {
        VStack(alignment: .leading, spacing: DS.gap) {
            Text(L10n.findings).font(.headline)
            Card {
                ForEach(items) { f in
                    HStack(alignment: .firstTextBaseline, spacing: DS.gap) {
                        Image(systemName: f.severity == "error" ? "xmark.octagon.fill" : "exclamationmark.triangle.fill")
                            .foregroundStyle(f.severity == "error" ? DS.bad : DS.warn)
                        Text(f.target).font(.callout.weight(.medium))
                        Text(f.message).font(.callout).foregroundStyle(.secondary)
                    }
                }
            }
        }
    }

    private func changes(_ items: [PlannedChange]) -> some View {
        VStack(alignment: .leading, spacing: DS.gap) {
            Text(L10n.pendingChanges).font(.headline)
            Card {
                ForEach(items) { c in
                    HStack(spacing: DS.gap) {
                        Chip(text: c.action, tint: c.action == "delete" ? DS.bad : .accentColor)
                        Text(c.detail).font(.callout)
                        Spacer()
                        if let path = c.path {
                            Text((path as NSString).abbreviatingWithTildeInPath)
                                .font(.caption).foregroundStyle(.tertiary).lineLimit(1).truncationMode(.middle)
                        }
                    }
                }
            }
        }
    }
}
