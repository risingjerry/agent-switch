// [INPUT]: SwiftUI, AppKit, AppState
// [OUTPUT]: SecretsView — secret names, who uses them, add/replace/reveal/delete
// [POS]: Features/Secrets — the Secrets page
// [PROTOCOL]: When this file changes, update this header, then check CLAUDE.md

import AppKit
import SwiftUI

struct SecretsView: View {
    @Environment(AppState.self) private var state
    @State private var editing: SecretDraft?
    @State private var revealed: [String: String] = [:]
    @State private var pendingDelete: String?
    @State private var query = ""

    private struct Item: Identifiable {
        let name: String
        let stored: Bool
        var id: String { name }
    }

    private var items: [Item] {
        let stored = Set(state.secretNames)
        let names = stored.union(state.missingSecrets).sorted()
        return names.filter { query.isEmpty || $0.localizedCaseInsensitiveContains(query) }
            .map { Item(name: $0, stored: stored.contains($0)) }
    }

    var body: some View {
        let missing = items.filter { !$0.stored }
        let inUse = items.filter { $0.stored && !state.consumers(of: $0.name).isEmpty }
        let other = items.filter { $0.stored && state.consumers(of: $0.name).isEmpty }
        List {
            if !missing.isEmpty {
                Section(L10n.missingSecrets) { ForEach(missing) { row($0) } }
            }
            if !inUse.isEmpty {
                Section(L10n.t("Used by MCP servers", "MCP 在用")) { ForEach(inUse) { row($0) } }
            }
            Section {
                ForEach(other) { row($0) }
            } header: {
                Text(L10n.t("Other secrets · \(other.count)", "其他密钥 · \(other.count)"))
            } footer: {
                Text(L10n.secretsFooter).font(.callout).foregroundStyle(.secondary).padding(.top, DS.gap)
            }
        }
        .listStyle(.inset)
        .navigationTitle(L10n.secrets)
        .searchable(text: $query, placement: .toolbar, prompt: L10n.search)
        .toolbar {
            Button { editing = SecretDraft(name: "", isNew: true) } label: { Label(L10n.newSecret, systemImage: "plus") }
                .help(L10n.newSecret)
        }
        .sheet(item: $editing) { draft in SecretSheet(draft: draft) }
        .confirmationDialog(L10n.t("Delete \(pendingDelete ?? "")?", "删除 \(pendingDelete ?? "")？"),
                            isPresented: Binding(get: { pendingDelete != nil }, set: { if !$0 { pendingDelete = nil } })) {
            Button(L10n.delete, role: .destructive) {
                if let name = pendingDelete { Task { await state.deleteSecret(name) } }
            }
        }
    }

    private func row(_ item: Item) -> some View {
        let users = state.consumers(of: item.name)
        return HStack(spacing: DS.pad) {
            Image(systemName: item.stored ? "key.horizontal.fill" : "key.slash")
                .foregroundStyle(item.stored ? Color.accentColor : DS.bad)
                .frame(width: 20)
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    Text(item.name).font(.body.monospaced().weight(.medium))
                    if !item.stored { Chip(text: L10n.missingSecrets, tint: DS.bad) }
                }
                if let value = revealed[item.name] {
                    Text(value).font(.caption.monospaced()).textSelection(.enabled).lineLimit(1).truncationMode(.middle)
                } else if !users.isEmpty {
                    Text("\(L10n.usedBy): \(users.joined(separator: ", "))")
                        .font(.caption).foregroundStyle(.secondary).lineLimit(1)
                }
            }
            Spacer()
            if item.stored {
                Menu {
                    Button(L10n.reveal) { Task { await reveal(item.name) } }
                    Button(L10n.copy) { Task { await copy(item.name) } }
                    Button(L10n.replace) { editing = SecretDraft(name: item.name, isNew: false) }
                    Divider()
                    Button(L10n.delete, role: .destructive) { pendingDelete = item.name }
                } label: {
                    Image(systemName: "ellipsis.circle")
                }
                .menuStyle(.borderlessButton)
                .fixedSize()
            } else {
                Button(L10n.add) { editing = SecretDraft(name: item.name, isNew: false) }
            }
        }
        .padding(.vertical, 3)
    }

    private func reveal(_ name: String) async {
        do {
            revealed[name] = try await state.service.revealSecret(name: name)
            try? await Task.sleep(for: .seconds(10))
            revealed[name] = nil
        } catch {
            state.error = error.localizedDescription
        }
    }

    private func copy(_ name: String) async {
        do {
            let value = try await state.service.revealSecret(name: name)
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(value, forType: .string)
        } catch {
            state.error = error.localizedDescription
        }
    }
}

private struct SecretDraft: Identifiable {
    var name: String
    let isNew: Bool
    var id: String { isNew ? "__new__" : name }
}

private struct SecretSheet: View {
    @Environment(AppState.self) private var state
    @Environment(\.dismiss) private var dismiss
    @State var draft: SecretDraft
    @State private var value = ""

    private var validName: Bool {
        draft.name.range(of: "^[A-Z][A-Z0-9_]*$", options: .regularExpression) != nil
    }

    var body: some View {
        VStack(alignment: .leading, spacing: DS.pad) {
            Text(draft.isNew ? L10n.newSecret : draft.name).font(.title2.weight(.semibold))
            Form {
                if draft.isNew {
                    TextField(L10n.secretName, text: $draft.name)
                        .font(DS.mono)
                        .onChange(of: draft.name) { _, v in draft.name = v.uppercased() }
                }
                SecureField(L10n.secretValue, text: $value)
                    .font(DS.mono)
            }
            .formStyle(.grouped)
            HStack {
                Spacer()
                Button(L10n.cancel) { dismiss() }
                Button(L10n.save) {
                    let (name, secret) = (draft.name, value)
                    value = ""
                    dismiss()
                    Task { await state.setSecret(name, secret) }
                }
                .buttonStyle(.borderedProminent)
                .keyboardShortcut(.defaultAction)
                .disabled(!validName || value.isEmpty || value.contains("\n"))
            }
        }
        .padding(DS.page)
        .frame(width: 460)
    }
}
