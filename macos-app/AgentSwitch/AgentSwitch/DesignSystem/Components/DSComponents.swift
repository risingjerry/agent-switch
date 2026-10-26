// [INPUT]: SwiftUI, DS tokens
// [OUTPUT]: Card, StatTile, Chip, StatusDot, PageHeader, ErrorBanner
// [POS]: DesignSystem/Components — shared building blocks for feature views
// [PROTOCOL]: When this file changes, update this header, then check CLAUDE.md

import SwiftUI

struct Card<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: DS.gap) { content }
            .padding(DS.pad)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(DS.card, in: RoundedRectangle(cornerRadius: DS.radius, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: DS.radius, style: .continuous).strokeBorder(DS.stroke))
    }
}

struct StatTile: View {
    let symbol: String
    let title: String
    let value: String
    let detail: String
    var tint: Color = .accentColor

    var body: some View {
        Card {
            Label(title, systemImage: symbol)
                .font(.callout.weight(.medium))
                .foregroundStyle(tint)
            Text(value)
                .font(.system(size: 28, weight: .semibold, design: .rounded))
                .monospacedDigit()
                .contentTransition(.numericText())
            Text(detail)
                .font(.callout)
                .foregroundStyle(.secondary)
                .lineLimit(2)
        }
    }
}

struct Chip: View {
    let text: String
    var tint: Color = .secondary

    var body: some View {
        Text(text)
            .font(.caption.weight(.medium))
            .padding(.horizontal, 7)
            .padding(.vertical, 2)
            .foregroundStyle(tint)
            .background(tint.opacity(0.12), in: Capsule())
    }
}

struct StatusDot: View {
    let color: Color
    var body: some View {
        Circle().fill(color).frame(width: 8, height: 8)
    }
}

struct PageHeader<Trailing: View>: View {
    let title: String
    var subtitle: String?
    @ViewBuilder var trailing: Trailing

    var body: some View {
        HStack(alignment: .firstTextBaseline) {
            VStack(alignment: .leading, spacing: 4) {
                Text(title).font(.largeTitle.weight(.semibold))
                if let subtitle {
                    Text(subtitle).font(.callout).foregroundStyle(.secondary)
                }
            }
            Spacer()
            trailing
        }
    }
}

extension PageHeader where Trailing == EmptyView {
    init(title: String, subtitle: String? = nil) {
        self.init(title: title, subtitle: subtitle) { EmptyView() }
    }
}

/// Globe menu that switches the UI language live (system / 中文 / English).
struct LanguageMenu: View {
    @AppStorage(L10n.languageKey) private var language = "system"

    var body: some View {
        Menu {
            Picker(L10n.language, selection: $language) {
                Text(L10n.followSystem).tag("system")
                Text("中文").tag("zh")
                Text("English").tag("en")
            }
            .pickerStyle(.inline)
        } label: {
            Label(L10n.language, systemImage: "globe")
        }
        .help(L10n.language)
    }
}

/// Pretty agent names for app ids used by the CLI.
enum AppNames {
    static func label(_ id: String) -> String {
        switch id {
        case "claude": return "Claude Code"
        case "claude_desktop": return "Claude Desktop"
        case "codex": return "Codex"
        case "hermes": return "Hermes"
        case "deepseek": return "DeepSeek Harness"
        default: return id
        }
    }

    static func short(_ id: String) -> String {
        switch id {
        case "claude": return "Claude"
        case "claude_desktop": return "Desktop"
        case "codex": return "Codex"
        case "hermes": return "Hermes"
        case "deepseek": return "DeepSeek"
        default: return id
        }
    }
}
