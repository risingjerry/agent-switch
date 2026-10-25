// [INPUT]: SwiftUI, AppState, DSComponents, DSTokens, L10n
// [OUTPUT]: SettingsView — app preferences and configuration paths
// [POS]: Features/Settings — allows runtime path inspection and preferences
// [PROTOCOL]: When this file changes, update this header, then check CLAUDE.md

import SwiftUI

struct SettingsView: View {
    @EnvironmentObject var appState: AppState
    @AppStorage("includeCCSwitch") private var includeCCSwitch = false

    var body: some View {
        DSPage(title: L10n.settings, subtitle: pageSubtitle, badges: pageBadges) {
            settingsSummary

            generalSection
            runtimePathsSection
            targetConfigsSection
            aboutSection
        }
    }

    private var pageSubtitle: String? {
        appState.lastRefresh.map { "\(L10n.lastRefreshed)\($0.formatted(date: .omitted, time: .shortened))" }
    }

    private var pageBadges: [DSPageBadge] {
        [
            DSPageBadge(text: L10n.ready, tone: .good),
            DSPageBadge(text: "v\(AppVersion.current)", tone: .neutral),
        ]
    }

    private var settingsSummary: some View {
        LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: DSSpacing.md), count: 4), spacing: DSSpacing.md) {
            DSMetricCard(
                label: L10n.config,
                value: L10n.ready,
                note: "~/.config/agent-switch/config.json"
            )
            DSMetricCard(
                label: L10n.secretsPath,
                value: L10n.ready,
                note: "~/.config/agent-switch/secrets.env"
            )
            DSMetricCard(
                label: L10n.targetConfigs,
                value: "4",
                note: "Claude Code / Desktop / Codex / Hermes"
            )
            DSMetricCard(
                label: L10n.version,
                value: AppVersion.current,
                note: "github.com/risingjerry/agent-switch"
            )
        }
    }

    private var generalSection: some View {
        DSCard {
            VStack(alignment: .leading, spacing: DSSpacing.lg) {
                Text(L10n.general)
                    .font(DSTypography.heading)

                Toggle(L10n.includeCCSwitch, isOn: $includeCCSwitch)
                    .toggleStyle(.switch)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private var runtimePathsSection: some View {
        DSCard {
            VStack(alignment: .leading, spacing: DSSpacing.md) {
                Text(L10n.runtimePaths)
                    .font(DSTypography.heading)

                DSPathRow(label: L10n.config, path: "~/.config/agent-switch/config.json")
                Divider()
                DSPathRow(label: L10n.secretsPath, path: "~/.config/agent-switch/secrets.env")
                Divider()
                DSPathRow(label: L10n.wrappers, path: "~/.config/agent-switch/mcp/bin/")
                Divider()
                DSPathRow(label: L10n.backups, path: "~/.config/agent-switch/backups/")
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private var targetConfigsSection: some View {
        DSCard {
            VStack(alignment: .leading, spacing: DSSpacing.md) {
                Text(L10n.targetConfigs)
                    .font(DSTypography.heading)

                DSPathRow(label: "Claude Code", path: "~/.claude.json")
                Divider()
                DSPathRow(label: "Claude Desktop", path: "~/Library/Application Support/Claude/claude_desktop_config.json")
                Divider()
                DSPathRow(label: "Codex", path: "~/.codex/config.toml")
                Divider()
                DSPathRow(label: "Hermes", path: "~/.hermes/config.yaml")
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private var aboutSection: some View {
        DSCard {
            VStack(alignment: .leading, spacing: DSSpacing.md) {
                Text(L10n.about)
                    .font(DSTypography.heading)

                DSInfoRow(label: L10n.version, value: AppVersion.current)
                Divider()
                HStack(alignment: .firstTextBaseline, spacing: DSSpacing.lg) {
                    Text(L10n.repository)
                        .font(DSTypography.body)
                        .foregroundStyle(DSColor.textSecondary)
                        .frame(width: 140, alignment: .leading)
                    Link("github.com/risingjerry/agent-switch", destination: URL(string: "https://github.com/risingjerry/agent-switch")!)
                        .font(DSTypography.mono)
                    Spacer()
                }
                .padding(.vertical, DSSpacing.xs)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}
