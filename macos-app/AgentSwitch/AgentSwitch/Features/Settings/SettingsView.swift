// [INPUT]: SwiftUI, AppState, L10n
// [OUTPUT]: SettingsView — language, binary location and usage window
// [POS]: Features/Settings — the Settings scene (⌘,)
// [PROTOCOL]: When this file changes, update this header, then check CLAUDE.md

import SwiftUI

struct SettingsView: View {
    @Environment(AppState.self) private var state
    @AppStorage(L10n.languageKey) private var language = "system"
    @AppStorage(AgentSwitchService.cliPathKey) private var cliPath = ""

    var body: some View {
        @Bindable var state = state
        Form {
            Section {
                Picker(L10n.language, selection: $language) {
                    Text(L10n.followSystem).tag("system")
                    Text("中文").tag("zh")
                    Text("English").tag("en")
                }
                Text(L10n.restartHint).font(.caption).foregroundStyle(.secondary)
            }
            Section {
                TextField(L10n.cliPath, text: $cliPath, prompt: Text("~/.local/bin/agent-switch"))
                    .font(DS.mono)
                Text(L10n.cliPathHint).font(.caption).foregroundStyle(.secondary)
            }
            Section {
                Picker(L10n.usageWindow, selection: $state.usageDays) {
                    ForEach([7, 30, 60, 90], id: \.self) { Text(L10n.days($0)).tag($0) }
                }
            }
            Section {
                LabeledContent(L10n.version, value: AppVersion.current)
            }
        }
        .formStyle(.grouped)
        .frame(width: 460)
        .fixedSize(horizontal: false, vertical: true)
    }
}
