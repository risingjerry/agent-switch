// [INPUT]: SwiftUI, AppState, L10n
// [OUTPUT]: Page enum and the sidebar with a live health footer
// [POS]: Navigation — drives ContentView's detail column
// [PROTOCOL]: When this file changes, update this header, then check CLAUDE.md

import SwiftUI

enum Page: String, CaseIterable, Identifiable {
    case overview, mcp, skills, secrets, clis
    var id: String { rawValue }

    var title: String {
        switch self {
        case .overview: L10n.overview
        case .mcp: L10n.mcp
        case .skills: L10n.skills
        case .secrets: L10n.secrets
        case .clis: L10n.clis
        }
    }

    var symbol: String {
        switch self {
        case .overview: "gauge.with.dots.needle.33percent"
        case .mcp: "point.3.connected.trianglepath.dotted"
        case .skills: "sparkles.rectangle.stack"
        case .secrets: "key.horizontal"
        case .clis: "terminal"
        }
    }
}

struct SidebarView: View {
    @Environment(AppState.self) private var state
    @Binding var selection: Page

    var body: some View {
        List(selection: $selection) {
            ForEach(Page.allCases) { page in
                Label(page.title, systemImage: page.symbol)
                    .badge(badge(for: page))
                    .tag(page)
            }
        }
        .navigationSplitViewColumnWidth(min: 190, ideal: 210)
        .safeAreaInset(edge: .bottom) { footer }
    }

    private func badge(for page: Page) -> Int {
        switch page {
        case .overview: state.pendingCount
        case .secrets: state.missingSecrets.count
        default: 0
        }
    }

    private var footer: some View {
        health
        .padding(.horizontal, DS.pad)
        .padding(.vertical, 12)
    }

    private var health: some View {
        HStack(spacing: DS.gap) {
            StatusDot(color: state.doctor == nil ? DS.quiet : (state.doctor!.blocked ? DS.bad : (state.needsSync ? DS.warn : DS.good)))
            Text(statusText).font(.callout).foregroundStyle(.secondary).lineLimit(1)
            Spacer()
            if state.syncing || state.busy {
                ProgressView().controlSize(.small)
            }
        }
    }

    private var statusText: String {
        guard let doctor = state.doctor else { return L10n.checking }
        if doctor.blocked { return L10n.blocked }
        return state.needsSync ? L10n.pending(state.pendingCount) : L10n.allGood
    }
}
