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
                AdviceCard(advice: state.advice)
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
        .task {
            if state.usage == nil { await state.loadUsage() }
            if state.advice == nil { await state.loadAdvice() }
        }
    }

    private var hero: some View {
        HStack(spacing: DS.pad) {
            Image(systemName: heroSymbol)
                .font(.system(size: 34, weight: .medium))
                .foregroundStyle(heroColor)
                .symbolEffect(.pulse, isActive: state.syncing)
            VStack(alignment: .leading, spacing: 4) {
                Text(heroTitle).font(.title.weight(.semibold))
                Text(L10n.t("Secrets, MCP servers and Skills for Claude Code, Codex, DeepSeek Harness and Hermes — in one place.",
                            "Claude Code、Codex、DeepSeek Harness、Hermes 的密钥、MCP 和 Skill，统一在这里管理。"))
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
        let missing = state.missingSecrets.count
        return LazyVGrid(columns: [GridItem(.adaptive(minimum: 210), spacing: DS.pad)], spacing: DS.pad) {
            tile(.mcp) {
                StatTile(symbol: Page.mcp.symbol, title: L10n.mcp, value: "\(enabled.count - hub)",
                         detail: L10n.t("resident · \(hub + state.mcps.count - enabled.count) on demand",
                                        "个常驻 · \(hub + state.mcps.count - enabled.count) 个按需"))
            }
            tile(.skills) {
                let all = state.skills?.skills.filter { $0.status != "missing" } ?? []
                let resident = all.filter { $0.status == "global" || $0.status == "project" }.count
                StatTile(symbol: Page.skills.symbol, title: L10n.skills, value: "\(resident)",
                         detail: L10n.t("resident · \(all.count - resident) on demand",
                                        "个常驻 · \(all.count - resident) 个按需"))
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
                            Text(DS.tilde(path))
                                .font(.caption).foregroundStyle(.tertiary).lineLimit(1).truncationMode(.middle)
                        }
                    }
                }
            }
        }
    }
}

/// What Fusebox suggests changing. People read it; agents apply it.
private struct AdviceCard: View {
    let advice: Advice?
    @State private var copied = false

    var body: some View {
        VStack(alignment: .leading, spacing: DS.gap) {
            HStack {
                Text(L10n.suggestions).font(.headline)
                if let n = advice?.suggestions.count, n > 0 {
                    Text("\(n)").font(.headline).foregroundStyle(.secondary)
                }
                Spacer()
            }
            Card {
                if let advice {
                    if advice.suggestions.isEmpty {
                        Label(L10n.noSuggestions, systemImage: "checkmark.circle").foregroundStyle(.secondary)
                    } else {
                        ForEach(Array(advice.suggestions.enumerated()), id: \.element.id) { index, s in
                            if index > 0 { Divider() }
                            row(s)
                        }
                    }
                } else {
                    ProgressView().frame(maxWidth: .infinity)
                }
                if advice.map({ !$0.suggestions.isEmpty }) ?? false {
                    Divider()
                    prompt
                }
            }
        }
    }

    private func row(_ s: Advice.Suggestion) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Image(systemName: symbol(s.kind)).foregroundStyle(.secondary).frame(width: 18)
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    Text(s.title).font(.body.weight(.medium))
                    if s.needsHuman { Chip(text: L10n.needsYou, tint: DS.warn) }
                }
                Text(s.reason).font(.callout).foregroundStyle(.secondary).lineLimit(2)
                if let items = s.items, !items.isEmpty {
                    Text(items.joined(separator: " · ")).font(.caption).foregroundStyle(.tertiary).lineLimit(1)
                }
            }
        }
        .padding(.vertical, 2)
        .help(s.commands.joined(separator: "\n"))
    }

    private var prompt: some View {
        VStack(alignment: .leading, spacing: 6) {
            Label(L10n.askAgent, systemImage: "sparkles").font(.callout.weight(.medium))
            Text(L10n.askAgentHint).font(.caption).foregroundStyle(.secondary)
            HStack(spacing: DS.gap) {
                Text(L10n.askAgentPrompt)
                    .font(.callout)
                    .textSelection(.enabled)
                    .padding(.horizontal, 10)
                    .padding(.vertical, 7)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(Color.primary.opacity(0.05), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
                Button(copied ? L10n.copied : L10n.copyPrompt) {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(L10n.askAgentPrompt, forType: .string)
                    copied = true
                    Task {
                        try? await Task.sleep(for: .seconds(2))
                        copied = false
                    }
                }
            }
        }
    }

    private func symbol(_ kind: String) -> String {
        switch kind {
        case "mcp": Page.mcp.symbol
        case "skill": Page.skills.symbol
        case "secret": Page.secrets.symbol
        default: "arrow.triangle.2.circlepath"
        }
    }
}
