# Fusebox macOS App — CLAUDE.md

## Architecture

Native SwiftUI app (macOS 14+). All data comes from the Rust `fusebox`
binary: `AgentSwitchService` runs it with `--json` and decodes the output into
`Models/AgentModels.swift`. The app never reads or writes secrets itself;
`secret set` goes through stdin and `secret get` through a private FIFO bound
to fd 3. Every write is followed by `fusebox reconcile`.

UI text is bilingual: `Resources/L10n.swift` holds every string as
`t("English", "中文")`. The language follows macOS on first launch and can be
overridden in Settings (stored as `language` = `system` | `zh` | `en`).

## Directory Structure

```
macos-app/AgentSwitch/AgentSwitch/
├── AgentSwitchApp.swift              # @main App: window, Settings, ⌘1–⌘5 / ⌘R / ⇧⌘S
├── ContentView.swift                 # NavigationSplitView + toolbar + error alert
├── Navigation/SidebarView.swift      # Page enum, sidebar, health footer
├── Models/AgentModels.swift          # Decodable mirrors of the CLI JSON
├── Services/AgentSwitchService.swift # actor that runs the binary
├── Services/AppState.swift           # @Observable state and actions
├── DesignSystem/                     # DS tokens + Card/StatTile/Chip/StatusDot
├── Resources/L10n.swift              # zh/en strings
└── Features/
    ├── Dashboard/DashboardView.swift # Overview
    ├── Agents/AgentsView.swift       # AgentsStrip used by Overview
    ├── Tools/ToolsView.swift         # MCP list, mode switch, editor, import
    ├── Skills/SkillsView.swift       # usage table, library, clean-up sheet
    ├── Secrets/SecretsView.swift     # names, consumers, add/replace/reveal
    ├── CLIs/CLIsView.swift
    └── Settings/SettingsView.swift
```

The Xcode project lists files explicitly: adding a Swift file means adding it
to `AgentSwitch.xcodeproj` as well (SwiftPM picks it up automatically).

## Build

```bash
cd macos-app/AgentSwitch
swift build                     # quick compile check
xcodebuild -project AgentSwitch.xcodeproj -scheme AgentSwitch -configuration Release build
```
