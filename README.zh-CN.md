<p align="center">
  <a href="README.md">English</a> · <strong>简体中文</strong>
</p>

<p align="center">
  <img src="docs/assets/fusebox-icon.png" width="104" alt="Fusebox 图标">
</p>

<h1 align="center">Fusebox</h1>

<p align="center">
  <strong>AI Agent 要用的 MCP 和 Skill，统一放在一处，用到才加载。</strong><br>
  给 Claude Code、Codex、DeepSeek Harness、Claude Desktop、Hermes 用的统一目录、统一密钥库和按需启动的 MCP。
</p>

<p align="center">
  <a href="https://github.com/risingjerry/fusebox/actions/workflows/ci.yml"><img src="https://github.com/risingjerry/fusebox/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/risingjerry/fusebox/releases"><img src="https://img.shields.io/github/v/release/risingjerry/fusebox?color=111111" alt="Release"></a>
  <img src="https://img.shields.io/badge/macOS%2014%2B%20%C2%B7%20Linux%20CLI-111111" alt="macOS 14+ 与 Linux 命令行">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-111111.svg" alt="MIT 许可"></a>
  <a href="https://github.com/risingjerry/fusebox/stargazers"><img src="https://img.shields.io/github/stars/risingjerry/fusebox?style=flat&color=111111" alt="Stars"></a>
</p>

<p align="center">
  <img src="docs/demo.gif" width="760" alt="fusebox find 用中文或英文为任务挑出合适的 Skill 和 MCP">
</p>

装了上百个 Skill、十几个 MCP 之后，每个 Agent 会话都要为它们全部买单：说明文字占满上下文，MCP 进程一直开着，每个 Agent 还各存一份配置和 API Key。

Fusebox 让每个 MCP 和 Skill 只有两种状态：

- **常驻**：每个会话都加载，因为你明确要求了。
- **按需**：其余全部。Agent 能看到名字，任务需要时自己查到、加载；MCP 闲置后自动关掉。

它用一份登记表生成各个 Agent 的原生配置；密钥只存一个文件，只有声明了某个 Key 的 MCP 才能读到；还会从你自己的会话记录里统计到底用了什么。

## 安装

```bash
# macOS：命令行 + App
brew tap risingjerry/tap && brew trust --tap risingjerry/tap
brew install risingjerry/tap/fusebox
brew install --cask risingjerry/tap/fusebox

# 任何装了 Rust 的系统（命令行；App 需要 macOS + Xcode）
git clone https://github.com/risingjerry/fusebox && fusebox/scripts/install.sh
```

然后：

```bash
fusebox doctor                     # 看它发现了什么、准备写什么（不会写入）
fusebox reconcile                  # 写入各 Agent 的配置（先备份）
fusebox install skill owner/repo   # 也可以是 GitHub 链接或本地文件夹；按锁定的提交克隆
fusebox install mcp github --command npx --arg -y --arg @example/github-mcp@1.2.3 --secret GITHUB_TOKEN
printf %s "$TOKEN" | fusebox secret set --stdin GITHUB_TOKEN   # 密钥不会出现在命令参数或日志里
```

每条会改动配置的命令做完都会自动同步所有 Agent；各 Agent 的全局说明也要求它们通过 Fusebox 安装。
对任何 Agent 说"帮我装这个 Skill：<链接>"，它都会装进 Fusebox，而不是只装进某一个 Agent 的目录。

新加的 MCP 和 Skill 默认**按需**。要常驻：`fusebox mcp direct ID` 或 `fusebox skills enable global NAME`。

## 能做什么

| | |
|---|---|
| **一个目录** | `fusebox find "合并两个 PDF"` 把 Skill 和 MCP 放在一起排序，中英文都行，并优先你在当前项目里常用的。 |
| **按需启动的 MCP** | 每个 Agent 只挂一个 `agent-hub`。某个 MCP 第一次被调用才启动，只拿到它自己的密钥，闲置 5 分钟自动关闭。工具上百个的 MCP 只列名字，需要时再取详细定义。 |
| **名单而不是长篇说明** | Claude Code 只看到按需 Skill 的名字（`name-only`），最常用的 30 个附一句说明；Codex 和 DeepSeek Harness 在说明文件里拿到同样的名单。 |
| **一个密钥库** | `~/.config/fusebox/secrets.env`（权限 0600）。只能通过标准输入或文件描述符写入，不走命令参数；每个 MCP 的启动脚本只导出它声明的 Key。 |
| **用量与建议** | `fusebox skills usage` 读取 Claude Code、Codex、DeepSeek Harness 的会话记录；`fusebox advise` 据此给出具体命令：哪些该常驻、哪些只在某个项目用、哪些 Agent 已经找不到了。 |
| **可量化的检索效果** | `fusebox eval` 用你真实的"提问 → 选了哪个 Skill"记录来测 `find`，并保留历史，改坏了能马上看出来。 |
| **macOS App** | 中英双语的仪表盘。人负责看，改动交给 Agent。 |

<p align="center">
  <img src="docs/assets/fusebox-overview.png" width="760" alt="Fusebox App 概览页：MCP、Skill、密钥和 Agent 同步状态（演示数据）">
</p>

## 配合 Claude Code、Codex 等 Agent 使用

装上自带的 Skill，任何 Agent 都知道怎么用 Fusebox：

```bash
npx skills add risingjerry/fusebox        # 或把 skills/fusebox 复制到 ~/.claude/skills/
```

然后直接说：

- "找个能把这份大纲做成幻灯片的 Skill。"
- "我的 Skill 和 MCP 里哪些值得常驻？先给我看，我同意的再改。"
- "把 Brave Search MCP 按需加上，Key 我来粘贴。"

Agent 会调用 `fusebox find`、`fusebox advise --json` 以及 hub 的 `fusebox_servers` / `fusebox_call`。在本仓库里干活的 Agent 请读 [AGENTS.md](AGENTS.md)。

## 命令

| 命令 | 作用 |
|---|---|
| `fusebox status` / `doctor` | 配置偏差、缺失密钥、问题；不写入 |
| `fusebox reconcile [--dry-run]` | 写入原生配置、启动脚本和说明（带备份） |
| `fusebox find 关键词` | 为任务给 Skill 和 MCP 排序 |
| `fusebox install skill 来源` · `install mcp ID` | 从仓库/文件夹装 Skill，或装 MCP（远程用 `--url`）；默认按需 |
| `fusebox mcp list\|add\|set\|remove\|import` | 管理 MCP；`mcp hub ID` / `mcp direct ID` 切换按需 / 常驻 |
| `fusebox secret set --stdin NAME` · `list` · `delete` | 密钥库（只输出名字） |
| `fusebox skills add-source\|fetch\|enable\|disable\|move\|drop\|restore` | Skill 来源与状态 |
| `fusebox skills usage` · `advise` · `eval` | 用了什么、该改什么、`find` 准不准 |

所有读取类命令都支持 `--json`。完整帮助：`fusebox --help`。

## 工作原理

```
             fusebox reconcile
config.json ───────────────────▶ ~/.claude.json、~/.claude/settings.json、~/.claude/skills
secrets.env                      ~/.codex/config.toml + AGENTS.md
Skill Hub                        ~/.dsh/cordis.patch.yml + AGENTS.md、Claude Desktop、Hermes

Agent ──▶ agent-hub（一个 stdio MCP）──首次调用──▶ 启动脚本 ──▶ MCP（只拿自己的密钥）
                                      ◀──闲置 5 分钟── 关闭
```

- Fusebox 只管理 `agent-*` 开头的 MCP 条目和它自己标记的说明段落，这些文件里的其他内容原样保留。
- Skill 放在 Skill Hub 里（默认 `~/.config/fusebox/skill-hub`；可在 `config.json` 设 `skillHub` 或用环境变量 `SKILL_HUB_HOME` 换位置），git 来源锁定版本，可按项目分配。

## 按需加载会不会挑得更差？

我们测下来不会。用 112 条真实的"用户提问 → Agent 加载了哪个 Skill"记录测试：只给模型看名字时，正确答案出现在它前三个选择里的比例，和给它看全部完整说明时一样；名字加上最常用 30 个的一句话说明效果更好（53% 对 47–48%，各跑两轮），篇幅只有全部说明的三分之一。用 `fusebox eval` 可以在你自己的会话记录上测 `find`。

## 常见问题

**怎么不让 MCP 占满 Claude Code 的上下文？**
把它们放到 `agent-hub` 后面（`fusebox mcp hub ID`，新加的默认就是）。Claude Code 只看到工具名，用到才加载定义。

**怎么让 Claude Code 和 Codex 共用一套 MCP？**
用 `fusebox mcp add` 每个只加一次，`fusebox reconcile` 会同时写入两边配置；两边都通过 hub 访问所有按需 MCP。

**API Key 存在哪？Agent 能读到吗？**
存在 `~/.config/fusebox/secrets.env`（权限 0600）。原生配置里只引用启动脚本，脚本只导出该 MCP 声明的 Key。Agent 只能看到 Key 的名字，看不到值。

**怎么知道哪些 Skill 从来没用过？**
`fusebox skills usage --days 60` 和 `fusebox advise`。

**支持 Linux 和 Windows 吗？**
命令行在 Linux 上能编译并通过 CI；App 只支持 macOS。Windows 未测试。

**App 有签名吗？**
是临时签名，没有经过苹果公证（还没有 Apple Developer ID）。Homebrew 安装时会去掉隔离标记，并会提示这一点。介意的话可以从源码编译。

## 卸载

```bash
brew uninstall --cask fusebox; brew uninstall fusebox   # 或删除 ~/.local/bin/fusebox
```

在你删掉 `agent-*` 条目之前，各 Agent 的配置照常可用；Fusebox 改过的每个文件都备份在 `~/.config/fusebox/backups`。

## 限制

- 远程（HTTP/SSE）MCP 暂不管理，Fusebox 不会改动它们。
- Codex 没有"只显示名字"的 Skill 模式，所以 Fusebox 把按需名单写进它的说明文件。
- hub 不转发服务器发给客户端的请求（sampling、elicitation）。

---

如果 Fusebox 帮你省下了上下文，点个 ⭐ 能让更多人找到它。

[![Star History Chart](https://api.star-history.com/svg?repos=risingjerry/fusebox&type=Date)](https://star-history.com/#risingjerry/fusebox&Date)

MIT © risingjerry · 曾用名 Agent Switch。
