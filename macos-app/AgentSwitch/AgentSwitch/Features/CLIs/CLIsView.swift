// [INPUT]: SwiftUI, AppState, CLIInfo
// [OUTPUT]: CLIsView — installed command-line tools and their versions
// [POS]: Features/CLIs — the CLIs page
// [PROTOCOL]: When this file changes, update this header, then check CLAUDE.md

import SwiftUI

struct CLIsView: View {
    @Environment(AppState.self) private var state

    var body: some View {
        Table(state.clis.sorted { ($0.installed ? 0 : 1, $0.name) < ($1.installed ? 0 : 1, $1.name) }) {
            TableColumn(L10n.clis) { cli in
                HStack(spacing: DS.gap) {
                    StatusDot(color: cli.installed ? DS.good : DS.quiet)
                    Text(cli.name).foregroundStyle(cli.installed ? .primary : .secondary)
                }
            }
            .width(min: 140, ideal: 170)
            TableColumn(L10n.version) { cli in
                Text(cli.version ?? (cli.installed ? "—" : L10n.notInstalled))
                    .foregroundStyle(.secondary).lineLimit(1)
            }
            .width(min: 160, ideal: 260)
            TableColumn(L10n.path) { cli in
                Text(cli.path.map { ($0 as NSString).abbreviatingWithTildeInPath } ?? "")
                    .font(.caption.monospaced()).foregroundStyle(.secondary).lineLimit(1).truncationMode(.middle)
            }
            TableColumn(L10n.t("Installed via", "安装方式")) { cli in
                Text(cli.manager).foregroundStyle(.secondary)
            }
            .width(min: 110, ideal: 140)
        }
        .navigationTitle(L10n.clis)
        .overlay { if state.clis.isEmpty { ProgressView() } }
        .task { if state.clis.isEmpty { await state.loadCLIs() } }
    }
}
