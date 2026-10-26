// [INPUT]: SwiftUI, AgentInfo, DS components
// [OUTPUT]: AgentsStrip — per-agent sync state shown on the Overview page
// [POS]: Features/Agents — embedded by DashboardView
// [PROTOCOL]: When this file changes, update this header, then check CLAUDE.md

import SwiftUI

struct AgentsStrip: View {
    let agents: [AgentInfo]

    var body: some View {
        VStack(alignment: .leading, spacing: DS.gap) {
            Text(L10n.agents).font(.headline)
            HStack(spacing: DS.pad) {
                ForEach(agents) { agent in
                    Card {
                        HStack(spacing: DS.gap) {
                            StatusDot(color: color(agent))
                            Text(agent.name).font(.body.weight(.medium))
                        }
                        Text(state(agent)).font(.callout).foregroundStyle(.secondary)
                        Text(DS.tilde(agent.instructionPath))
                            .font(.caption)
                            .foregroundStyle(.tertiary)
                            .lineLimit(1)
                            .truncationMode(.middle)
                    }
                    .opacity(agent.detected ? 1 : 0.55)
                    .help(agent.configPath)
                }
            }
        }
    }

    private func state(_ a: AgentInfo) -> String {
        if !a.detected { return L10n.notDetected }
        if a.inSync { return L10n.synced }
        return a.managed ? L10n.needsSync : L10n.unmanaged
    }

    private func color(_ a: AgentInfo) -> Color {
        if !a.detected { return DS.quiet }
        return a.inSync ? DS.good : DS.warn
    }
}
