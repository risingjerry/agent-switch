// [INPUT]: SwiftUI
// [OUTPUT]: DS — spacing, radius and semantic colours on top of system colours
// [POS]: DesignSystem/Tokens — consumed by components and feature views
// [PROTOCOL]: When this file changes, update this header, then check CLAUDE.md

import SwiftUI

/// The app leans on native macOS materials and system colours so it follows
/// light/dark mode, accent colour and accessibility settings for free.
enum DS {
    /// `~/…` for paths under the home the CLI runs with ($HOME), so a
    /// demo or custom home reads the same as the real one.
    static func tilde(_ path: String) -> String {
        let home = ProcessInfo.processInfo.environment["HOME"] ?? NSHomeDirectory()
        if path == home { return "~" }
        return path.hasPrefix(home + "/") ? "~" + path.dropFirst(home.count) : (path as NSString).abbreviatingWithTildeInPath
    }

    static let gap: CGFloat = 8
    static let pad: CGFloat = 16
    static let page: CGFloat = 24
    static let radius: CGFloat = 12

    static let good = Color.green
    static let warn = Color.orange
    static let bad = Color.red
    static let quiet = Color.secondary
    static let hub = Color.indigo

    static let card = Color(nsColor: .controlBackgroundColor)
    static let stroke = Color.primary.opacity(0.08)

    static let mono = Font.system(.callout, design: .monospaced)
}
