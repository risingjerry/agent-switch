// [INPUT]: SwiftUI, AppState, ContentView, SettingsView
// [OUTPUT]: App entry with the main window, Settings scene and menu commands
// [POS]: Root — the only @main type
// [PROTOCOL]: When this file changes, update this header, then check CLAUDE.md

import SwiftUI

@main
struct AgentSwitchApp: App {
    @State private var state = AppState()

    var body: some Scene {
        WindowGroup(L10n.appName, id: "main") {
            ContentView()
                .environment(state)
                .frame(minWidth: 900, minHeight: 600)
        }
        .defaultSize(width: 1180, height: 780)
        .windowToolbarStyle(.unified)
        .commands {
            CommandGroup(replacing: .newItem) {}
            CommandGroup(after: .sidebar) {
                ForEach(Array(Page.allCases.enumerated()), id: \.element) { index, page in
                    Button(page.title) { state.page = page }
                        .keyboardShortcut(KeyEquivalent(Character("\(index + 1)")), modifiers: .command)
                }
            }
            CommandGroup(after: .toolbar) {
                Button(L10n.refresh) { Task { await state.refresh() } }
                    .keyboardShortcut("r")
                Button(L10n.sync) { Task { await state.sync() } }
                    .keyboardShortcut("s", modifiers: [.command, .shift])
            }
        }

        Settings {
            SettingsView()
                .environment(state)
        }
    }
}
