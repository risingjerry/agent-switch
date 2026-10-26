// [INPUT]: SwiftUI, AppState, SidebarView, feature views
// [OUTPUT]: ContentView — split view, toolbar and error alert
// [POS]: Root view of the main window
// [PROTOCOL]: When this file changes, update this header, then check CLAUDE.md

import SwiftUI

struct ContentView: View {
    @Environment(AppState.self) private var state
    @AppStorage(L10n.languageKey) private var language = "system"

    var body: some View {
        @Bindable var state = state
        NavigationSplitView {
            SidebarView(selection: $state.page)
        } detail: {
            detail
                .toolbar {
                    ToolbarItemGroup(placement: .primaryAction) {
                        Button {
                            Task { await state.refresh() }
                        } label: {
                            Label(L10n.refresh, systemImage: "arrow.clockwise")
                        }
                        .help(L10n.refresh)
                        Button {
                            Task { await state.sync() }
                        } label: {
                            Label(L10n.sync, systemImage: "arrow.triangle.2.circlepath")
                        }
                        .help(L10n.sync)
                        .disabled(state.syncing)
                    }
                }
        }
        .id(language)
        .task { await state.refresh() }
        .alert(
            L10n.t("Something went wrong", "出错了"),
            isPresented: Binding(get: { state.error != nil }, set: { if !$0 { state.error = nil } })
        ) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(state.error ?? "")
        }
    }

    @ViewBuilder private var detail: some View {
        @Bindable var state = state
        switch state.page {
        case .overview: DashboardView(page: $state.page)
        case .mcp: ToolsView()
        case .skills: SkillsView()
        case .secrets: SecretsView()
        case .clis: CLIsView()
        }
    }
}
