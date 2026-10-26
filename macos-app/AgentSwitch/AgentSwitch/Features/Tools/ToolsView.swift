// [INPUT]: SwiftUI, AppState, MCPServer, MCPDraft, DS components
// [OUTPUT]: ToolsView — MCP list with per-row mode switch, editor inspector, import sheet
// [POS]: Features/Tools — the MCP page
// [PROTOCOL]: When this file changes, update this header, then check CLAUDE.md

import SwiftUI

struct ToolsView: View {
    @Environment(AppState.self) private var state
    @State private var selection: MCPServer.ID?
    @State private var draft: MCPDraft?
    @State private var showImport = false
    @State private var query = ""

    private var rows: [MCPServer] {
        let all = state.mcps.sorted { ($0.enabled ? 0 : 1, $0.shortName) < ($1.enabled ? 0 : 1, $1.shortName) }
        guard !query.isEmpty else { return all }
        return all.filter { $0.shortName.localizedCaseInsensitiveContains(query) || $0.command.localizedCaseInsensitiveContains(query) }
    }

    var body: some View {
        Group {
            if state.mcps.isEmpty && !state.busy {
                ContentUnavailableView {
                    Label(L10n.noMCP, systemImage: Page.mcp.symbol)
                } actions: {
                    Button(L10n.newMCP) { draft = .new() }
                    Button(L10n.importMCPs) { showImport = true }
                }
            } else {
                List(selection: $selection) {
                    ForEach([MCPMode.direct, .hub], id: \.self) { mode in
                        let items = rows.filter { $0.effectiveMode == mode || (mode == .hub && $0.effectiveMode == .off) }
                        if !items.isEmpty {
                            Section {
                                ForEach(items) { server in
                                    MCPRow(server: server, missing: Set(state.missingSecrets)).tag(server.id)
                                }
                            } header: {
                                Text("\(mode.title) · \(items.count)")
                            }
                        }
                    }
                    Section {} footer: {
                        Text(L10n.modeHelp).font(.callout).foregroundStyle(.secondary)
                    }
                }
                .listStyle(.inset)
                .searchable(text: $query, placement: .toolbar, prompt: L10n.search)
            }
        }
        .navigationTitle(L10n.mcp)
        .toolbar {
            ToolbarItemGroup {
                Button { showImport = true } label: { Label(L10n.importMCPs, systemImage: "square.and.arrow.down") }
                    .help(L10n.importMCPs)
                Button { selection = nil; draft = .new() } label: { Label(L10n.newMCP, systemImage: "plus") }
                    .help(L10n.newMCP)
            }
        }
        .onChange(of: selection) { _, id in
            guard let id, let server = state.mcps.first(where: { $0.id == id }) else { return }
            Task { draft = MCPDraft(server, env: await state.service.envValues(id: server.id)) }
        }
        .inspector(isPresented: Binding(get: { draft != nil }, set: { if !$0 { draft = nil; selection = nil } })) {
            if let binding = Binding($draft) {
                MCPEditor(draft: binding, isNew: selection == nil) {
                    draft = nil
                    selection = nil
                }
                .inspectorColumnWidth(min: 340, ideal: 380, max: 520)
            }
        }
        .sheet(isPresented: $showImport) { ImportSheet() }
    }
}

extension MCPMode {
    var title: String {
        switch self {
        case .off: L10n.modeOff
        case .direct: L10n.modeDirect
        case .hub: L10n.modeHub
        }
    }

    var color: Color {
        switch self {
        case .off: Color.gray
        case .direct: DS.good
        case .hub: DS.hub
        }
    }
}

private struct MCPRow: View {
    @Environment(AppState.self) private var state
    let server: MCPServer
    let missing: Set<String>

    /// One readable line instead of the raw command: description, or the
    /// executable plus the first script/URL it runs.
    private var summary: String {
        if let d = server.description, !d.isEmpty, !d.hasPrefix("Imported from") { return d }
        let exe = (server.command as NSString).lastPathComponent
        let target = server.args.first { $0.contains("/") || $0.contains(".") }.map { arg -> String in
            if let url = URL(string: arg), let host = url.host { return host }
            return (arg as NSString).lastPathComponent
        }
        return [exe, target].compactMap { $0 }.joined(separator: " · ")
    }

    var body: some View {
        // Two states only; a legacy disabled entry reads as on demand.
        let mode = server.effectiveMode == .direct ? MCPMode.direct : .hub
        let lacking = server.requiredSecrets.filter { missing.contains($0) }
        HStack(spacing: 12) {
            Text(String(server.name.prefix(1)).uppercased())
                .font(.system(size: 14, weight: .semibold, design: .rounded))
                .frame(width: 30, height: 30)
                .foregroundStyle(mode == .off ? Color.secondary : Color.primary)
                .background(mode.color.opacity(0.18), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(server.name).font(.body.weight(.medium)).lineLimit(1)
                    if !lacking.isEmpty {
                        Image(systemName: "exclamationmark.circle.fill")
                            .foregroundStyle(DS.bad)
                            .help("\(L10n.missing): \(lacking.joined(separator: ", "))")
                    }
                }
                Text(summary).font(.callout).foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle)
            }
            Spacer(minLength: DS.pad)
            Text(server.apps.names.map(AppNames.short).joined(separator: " · "))
                .font(.caption).foregroundStyle(.tertiary).lineLimit(1)
            if let n = server.toolCount {
                Text("\(n) \(L10n.tools)").font(.caption).foregroundStyle(.tertiary).monospacedDigit()
            }
            Circle().fill(mode.color).frame(width: 8, height: 8)
            Menu {
                Picker(L10n.apps, selection: Binding(get: { mode }, set: { next in
                    Task { await state.setMode(server, next) }
                })) {
                    ForEach([MCPMode.direct, .hub], id: \.self) { m in
                        Text(m.title).tag(m)
                    }
                }
                .pickerStyle(.inline)
            } label: {
                Text(mode.title).font(.callout)
            }
            .menuStyle(.borderlessButton)
            .fixedSize()
            .frame(minWidth: 72, alignment: .leading)
            .disabled(state.syncing)
        }
        .padding(.vertical, 5)
        .opacity(mode == .off ? 0.7 : 1)
        .help(([server.command] + server.args).joined(separator: " "))
    }
}

private struct MCPEditor: View {
    @Environment(AppState.self) private var state
    @Binding var draft: MCPDraft
    let isNew: Bool
    let close: () -> Void
    @State private var confirmDelete = false

    var body: some View {
        Form {
            Section {
                if isNew {
                    TextField("ID", text: $draft.id, prompt: Text("my-server"))
                }
                TextField(L10n.t("Name", "名称"), text: $draft.name)
                TextField(L10n.description, text: $draft.description, axis: .vertical)
            }
            Section(L10n.command) {
                TextField(L10n.command, text: $draft.command, prompt: Text("npx"))
                    .font(DS.mono)
                TextField(L10n.arguments, text: lines($draft.args), prompt: Text(L10n.oneArgPerLine), axis: .vertical)
                    .font(DS.mono)
                    .lineLimit(2...8)
            }
            Section(L10n.requiredSecrets) {
                TextField(L10n.requiredSecrets, text: lines($draft.secrets), prompt: Text("OPENAI_API_KEY"), axis: .vertical)
                    .font(DS.mono)
                    .lineLimit(1...5)
                Text(L10n.t("Use ${NAME} in arguments to pass a secret on the command line.",
                            "需要放进参数时，写成 ${NAME}。"))
                    .font(.caption).foregroundStyle(.secondary)
            }
            Section(L10n.environment) {
                TextField(L10n.environment, text: envText, prompt: Text("NAME=value"), axis: .vertical)
                    .font(DS.mono)
                    .lineLimit(1...6)
            }
            Section(L10n.apps) {
                Toggle("Claude Code", isOn: $draft.apps.claude)
                Toggle("Claude Desktop", isOn: $draft.apps.claude_desktop)
                Toggle("Codex", isOn: $draft.apps.codex)
                Toggle("Hermes", isOn: $draft.apps.hermes)
                Toggle("DeepSeek Harness", isOn: $draft.apps.deepseek)
            }
            Section {
                HStack {
                    if !isNew {
                        Button(L10n.delete, role: .destructive) { confirmDelete = true }
                    }
                    Spacer()
                    Button(L10n.cancel, action: close)
                    Button(L10n.save) {
                        let saved = draft
                        close()
                        Task { await state.save(saved) }
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(draft.command.isEmpty || (isNew && draft.id.isEmpty))
                    .keyboardShortcut(.defaultAction)
                }
            }
        }
        .formStyle(.grouped)
        .confirmationDialog(L10n.t("Delete \(draft.name)?", "删除 \(draft.name)？"), isPresented: $confirmDelete) {
            Button(L10n.delete, role: .destructive) {
                guard let server = state.mcps.first(where: { $0.id == draft.id }) else { return }
                close()
                Task { await state.remove(server) }
            }
        }
    }

    private func lines(_ binding: Binding<[String]>) -> Binding<String> {
        Binding(
            get: { binding.wrappedValue.joined(separator: "\n") },
            set: { binding.wrappedValue = $0.split(separator: "\n", omittingEmptySubsequences: false).map(String.init).filter { !$0.isEmpty } }
        )
    }

    private var envText: Binding<String> {
        Binding(
            get: { draft.env.sorted { $0.key < $1.key }.map { "\($0.key)=\($0.value)" }.joined(separator: "\n") },
            set: { text in
                var env: [String: String] = [:]
                for line in text.split(separator: "\n") {
                    let parts = line.split(separator: "=", maxSplits: 1)
                    if parts.count == 2 { env[String(parts[0])] = String(parts[1]) }
                }
                draft.env = env
            }
        )
    }
}

private struct ImportSheet: View {
    @Environment(AppState.self) private var state
    @Environment(\.dismiss) private var dismiss
    @State private var preview: MCPImportPreview?
    @State private var failure: String?

    var body: some View {
        VStack(alignment: .leading, spacing: DS.pad) {
            Text(L10n.importMCPs.replacingOccurrences(of: "…", with: "")).font(.title2.weight(.semibold))
            Text(L10n.t("MCP servers already configured in Claude Code, Claude Desktop, Codex or Hermes are moved here. Inline keys go into the secret store; the original files are backed up.",
                        "把已经配置在 Claude Code、Claude Desktop、Codex、Hermes 里的 MCP 收进来统一管理。写在配置里的密钥会移进密钥库，原文件会先备份。"))
                .foregroundStyle(.secondary)
            if let preview {
                Card {
                    row(L10n.t("New", "新增"), preview.imported)
                    row(L10n.t("Merged", "合并"), preview.merged)
                    row(L10n.t("Secrets moved", "移入密钥库"), preview.secretNames)
                    if !preview.skipped.isEmpty {
                        row(L10n.t("Skipped (remote)", "跳过（远程类型）"), preview.skipped.map { "\($0.id) (\(AppNames.short($0.app)))" })
                    }
                }
            } else if let failure {
                Text(failure).foregroundStyle(DS.bad)
            } else {
                ProgressView()
            }
            HStack {
                Spacer()
                Button(L10n.cancel) { dismiss() }
                Button(L10n.t("Import", "导入")) {
                    dismiss()
                    Task { await state.importMCPs() }
                }
                .buttonStyle(.borderedProminent)
                .disabled((preview?.imported.isEmpty ?? true) && (preview?.merged.isEmpty ?? true))
            }
        }
        .padding(DS.page)
        .frame(width: 520)
        .task {
            do { preview = try await state.service.importPreview() } catch { failure = error.localizedDescription }
        }
    }

    private func row(_ title: String, _ items: [String]) -> some View {
        HStack(alignment: .firstTextBaseline) {
            Text(title).frame(width: 120, alignment: .leading).foregroundStyle(.secondary)
            Text(items.isEmpty ? "—" : items.joined(separator: ", ")).font(.callout.monospaced())
        }
    }
}
