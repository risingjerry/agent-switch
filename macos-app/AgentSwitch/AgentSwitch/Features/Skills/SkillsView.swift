// [INPUT]: SwiftUI, AppState, SkillUsage, SkillReport, PrunePlan
// [OUTPUT]: SkillsView — usage table, library list and the clean-up sheet
// [POS]: Features/Skills — the Skills page
// [PROTOCOL]: When this file changes, update this header, then check CLAUDE.md

import SwiftUI

struct SkillsView: View {
    @Environment(AppState.self) private var state
    @State private var tab = 0
    @State private var showCleanup = false
    @State private var query = ""
    @State private var order = [KeyPathComparator(\SkillUsage.Row.total, order: .reverse)]

    var body: some View {
        @Bindable var state = state
        VStack(spacing: 0) {
            header
            Divider()
            if tab == 0 { usageTable } else { library }
        }
        .navigationTitle(L10n.skills)
        .searchable(text: $query, placement: .toolbar, prompt: L10n.search)
        .toolbar {
            ToolbarItem(placement: .principal) {
                Picker("", selection: $tab) {
                    Text(L10n.usage).tag(0)
                    Text(L10n.library).tag(1)
                }
                .pickerStyle(.segmented)
                .fixedSize()
            }
        }
        .task { if state.usage == nil || state.usage?.days != state.usageDays { await state.loadUsage() } }
        .onChange(of: state.usageDays) { Task { await state.loadUsage() } }
        .sheet(isPresented: $showCleanup) { CleanupSheet() }
    }

    private var header: some View {
        @Bindable var state = state
        let rows = state.usage?.enabled ?? []
        let unused = rows.filter { $0.total == 0 }.count
        return HStack(alignment: .center, spacing: DS.pad) {
            VStack(alignment: .leading, spacing: 2) {
                if let usage = state.usage {
                    Text(L10n.unusedSummary(unused, rows.count, usage.days)).font(.headline)
                    Text(L10n.sessions(usage.sessions["claude"] ?? 0, usage.sessions["codex"] ?? 0, usage.sessions["deepseek"] ?? 0))
                        .font(.callout).foregroundStyle(.secondary)
                } else {
                    Text(L10n.checking).font(.headline)
                }
            }
            Spacer()
            if state.loadingUsage { ProgressView().controlSize(.small) }
            Picker(L10n.usageWindow, selection: $state.usageDays) {
                ForEach([7, 30, 60, 90], id: \.self) { Text(L10n.days($0)).tag($0) }
            }
            .fixedSize()
            Button(L10n.cleanUp) { showCleanup = true }
                .buttonStyle(.borderedProminent)
                .disabled(unused == 0)
        }
        .padding(.horizontal, DS.page)
        .padding(.vertical, DS.pad)
    }

    private var usageRows: [SkillUsage.Row] {
        let rows = state.usage?.enabled ?? []
        let filtered = query.isEmpty ? rows : rows.filter { $0.name.localizedCaseInsensitiveContains(query) }
        return filtered.sorted(using: order)
    }

    private var usageTable: some View {
        Table(usageRows, sortOrder: $order) {
            TableColumn(L10n.skills, value: \.name) { row in
                Text(row.name).foregroundStyle(row.total == 0 ? .secondary : .primary)
            }
            .width(min: 200, ideal: 260)
            TableColumn("Claude Code", value: \.claude) { row in UsageBar(value: row.claude, tint: .orange) }
                .width(min: 90, ideal: 110)
            TableColumn("Codex", value: \.codex) { row in UsageBar(value: row.codex, tint: .blue) }
                .width(min: 90, ideal: 110)
            TableColumn("DeepSeek", value: \.ds) { row in UsageBar(value: row.ds, tint: .indigo) }
                .width(min: 90, ideal: 110)
            TableColumn(L10n.lastUsed, value: \.lastUsed) { row in
                Text(row.lastUsed == "-" ? L10n.never : row.lastUsed).foregroundStyle(.secondary).monospacedDigit()
            }
            .width(min: 90, ideal: 100)
            TableColumn(L10n.size, value: \.listingChars) { row in
                Text("\(row.listingChars)").foregroundStyle(.secondary).monospacedDigit()
            }
            .width(min: 70, ideal: 80)
            TableColumn(L10n.profiles) { row in
                Text(row.profiles.joined(separator: ", ")).foregroundStyle(.secondary).lineLimit(1)
            }
        }
    }

    private var library: some View {
        let skills = (state.skills?.skills ?? []).filter {
            query.isEmpty || $0.name.localizedCaseInsensitiveContains(query) || $0.source.localizedCaseInsensitiveContains(query)
        }
        return List {
            ForEach(["global", "project", "dormant", "missing"], id: \.self) { status in
                let items = skills.filter { $0.status == status }
                if !items.isEmpty {
                    Section("\(statusTitle(status)) · \(items.count)") {
                        ForEach(items) { skill in
                            HStack {
                                Text(skill.name)
                                Spacer()
                                if !skill.profiles.isEmpty {
                                    Text(skill.profiles.joined(separator: ", ")).font(.caption).foregroundStyle(.secondary)
                                }
                                Chip(text: skill.source)
                            }
                            .contextMenu {
                                Button(L10n.t("Show in Finder", "在访达中显示")) {
                                    NSWorkspace.shared.selectFile(nil, inFileViewerRootedAtPath: skill.absolutePath)
                                }
                            }
                        }
                    }
                }
            }
        }
        .listStyle(.inset)
    }

    private func statusTitle(_ s: String) -> String {
        switch s {
        case "global": L10n.statusGlobal
        case "project": L10n.statusProject
        case "dormant": L10n.statusDormant
        default: L10n.statusMissing
        }
    }
}

private struct UsageBar: View {
    let value: Int
    let tint: Color

    var body: some View {
        HStack(spacing: 6) {
            Text("\(value)").monospacedDigit().frame(width: 26, alignment: .trailing)
                .foregroundStyle(value == 0 ? .tertiary : .primary)
            Capsule()
                .fill(tint.opacity(value == 0 ? 0.1 : 0.7))
                .frame(width: max(3, min(60, CGFloat(value) * 4)), height: 5)
        }
    }
}

private struct CleanupSheet: View {
    @Environment(AppState.self) private var state
    @Environment(\.dismiss) private var dismiss
    @State private var profile = "global"
    @State private var plan: PrunePlan?
    @State private var chosen: Set<String> = []
    @State private var failure: String?

    var body: some View {
        VStack(alignment: .leading, spacing: DS.pad) {
            Text(L10n.cleanupTitle).font(.title2.weight(.semibold))
            Text(L10n.cleanupHint).foregroundStyle(.secondary)
            Picker(L10n.profile, selection: $profile) {
                ForEach(state.skills?.profiles.map(\.id) ?? ["global"], id: \.self) { Text($0).tag($0) }
            }
            .fixedSize()
            if let plan {
                if plan.disabled.isEmpty {
                    Text(L10n.nothingToClean).foregroundStyle(.secondary).padding(.vertical)
                } else {
                    List(plan.disabled, id: \.self) { name in
                        Toggle(name, isOn: Binding(
                            get: { chosen.contains(name) },
                            set: { if $0 { chosen.insert(name) } else { chosen.remove(name) } }))
                    }
                    .frame(minHeight: 180, maxHeight: 320)
                }
                if !plan.protected.isEmpty {
                    DisclosureGroup(L10n.kept + " · \(plan.protected.count)") {
                        ForEach(plan.protected) { p in
                            HStack {
                                Text(p.name)
                                Spacer()
                                Text(p.reason).font(.caption).foregroundStyle(.secondary)
                            }
                        }
                    }
                }
            } else if let failure {
                Text(failure).foregroundStyle(DS.bad)
            } else {
                ProgressView().frame(maxWidth: .infinity)
            }
            HStack {
                Spacer()
                Button(L10n.cancel) { dismiss() }
                Button(L10n.turnOff(chosen.count)) {
                    let names = Array(chosen)
                    dismiss()
                    Task { await state.disableSkills(names, profile: profile) }
                }
                .buttonStyle(.borderedProminent)
                .disabled(chosen.isEmpty)
            }
        }
        .padding(DS.page)
        .frame(width: 520)
        .task(id: profile) {
            plan = nil
            failure = nil
            do {
                let next = try await state.service.prunePlan(profile: profile, days: state.usageDays)
                plan = next
                chosen = Set(next.disabled)
            } catch {
                failure = error.localizedDescription
            }
        }
    }
}
