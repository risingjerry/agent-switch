<p align="center">
  <a href="README.md">English</a> · <strong>简体中文</strong>
</p>

<p align="center">
  <img src="docs/assets/agent-switch-icon.png" width="112" alt="Agent Switch 应用图标">
</p>

<h1 align="center">Agent Switch</h1>

<p align="center">
  <strong>每个 MCP 只定义一次，每个密钥只保存一次，再安全同步到所有支持的本地 AI Agent。</strong>
</p>

<p align="center">
  <a href="https://github.com/risingjerry/agent-switch/actions/workflows/ci.yml"><img src="https://github.com/risingjerry/agent-switch/actions/workflows/ci.yml/badge.svg" alt="CI 状态"></a>
  <a href="https://www.apple.com/macos/"><img src="https://img.shields.io/badge/macOS-14%2B-111111?logo=apple" alt="需要 macOS 14 或更高版本"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-111111.svg" alt="MIT 许可证"></a>
</p>

<p align="center">
  <a href="https://github.com/risingjerry/agent-switch/stargazers"><img src="https://img.shields.io/github/stars/risingjerry/agent-switch?style=flat-square&label=stars" alt="GitHub stars"></a>
  <a href="https://github.com/risingjerry/agent-switch/forks"><img src="https://img.shields.io/github/forks/risingjerry/agent-switch?style=flat-square&label=forks" alt="GitHub forks"></a>
  <a href="https://github.com/risingjerry/agent-switch/commits/main"><img src="https://img.shields.io/github/commit-activity/t/risingjerry/agent-switch?style=flat-square&label=commits" alt="GitHub commit activity"></a>
  <a href="https://github.com/risingjerry/agent-switch/releases"><img src="https://img.shields.io/github/downloads/risingjerry/agent-switch/total?style=flat-square&label=downloads" alt="Release downloads"></a>
  <img src="https://visitor-badge.laobi.icu/badge?page_id=JNHFlow21.agent-switch&left_text=README%20visits" alt="README visit counter">
</p>

<p align="center"><img src="docs/demo.gif" width="820" alt="终端演示：用 stdin 存入凭据，agent-switch mcp add 定义一次 MCP，reconcile --dry-run 预览各 Agent 配置变更，应用后用 status 检查健康（示例数据为虚构）"></p>

<p align="center">
  <img src="docs/assets/agent-switch-dashboard-en.png" width="820" alt="Agent Switch 仪表板，展示健康状态、配置漂移、MCP 路由和密钥可用状态">
</p>

<p align="center">
  <sub>真实 macOS 仪表板使用隔离的示例配置，展示健康状态、配置漂移、路由和密钥是否就绪，但绝不显示密钥值。</sub>
</p>

Codex、Claude Code、Claude Desktop 和 Hermes 都维护着各自的 MCP
配置。把相同的服务器与密钥复制到每个客户端，不仅容易产生配置漂移，也会让密钥散落在多个文件中。

Agent Switch 用一个本地 MCP 注册表、一个私密凭据存储和可重复生成的 Agent
配置替代这些副本。

> [!IMPORTANT]
> Agent Switch 目前仍是 **Alpha 软件**。公开 App 使用 ad-hoc 签名，尚未经过
> Apple 公证，因此项目自己的 Homebrew Cask 会在安装完成后移除 Gatekeeper
> quarantine。安装前请先检查[公开源码](https://github.com/risingjerry/agent-switch)
> 和[发行校验和](https://github.com/risingjerry/agent-switch/releases/tag/v0.3.0)。

原生应用会跟随 macOS 的语言偏好，并完整提供**英文**与**简体中文**界面。

## 快速开始

### 前置条件

- macOS 14 或更高版本
- Homebrew 6.0 或更高版本

### 安装

复制执行下面这一条经过验证的命令：

```bash
brew tap risingjerry/tap && brew trust --tap risingjerry/tap && brew install --cask risingjerry/tap/agent-switch
```

Homebrew 会：

1. 注册并显式信任项目自己的 Tap；
2. 通过 `agent-switch-cli` 安装 `agent-switch` CLI；
3. 把通用架构原生 App 安装到 `/Applications/Agent Switch.app`；
4. 因为当前 Alpha 尚未公证，所以移除 Gatekeeper quarantine；
5. 不会自动接管 MCP，也不会改写原生 Agent 配置。

首次安装成功后应当看到：

```text
agent-switch --version  -> agent-switch 0.3.0
原生应用                  -> /Applications/Agent Switch.app
```

从“应用程序”打开 Agent Switch，或者运行：

```bash
open -a "Agent Switch"
agent-switch mcp import --dry-run --json
agent-switch doctor
```

在执行 `agent-switch mcp import --adopt` 或 `agent-switch reconcile`
之前，请先检查检测到的 MCP ID、目标应用和所需密钥的**名称**。

<details>
<summary>为什么使用 Homebrew，而不是 npm 或 NPX？</summary>

Agent Switch 是原生 macOS 应用加 Python CLI，Node.js 并不是它的运行时。
npm/NPX 只会引入无关依赖，最终仍然需要构建或下载 macOS 应用。

当前 Alpha 的发行链路是：

```text
ad-hoc 签名的发行产物 -> GitHub Releases -> 项目自己的 Homebrew Cask
```

Developer ID 签名和 Apple 公证仍然是稳定版目标。详情见[路线图](docs/roadmap.md)。

</details>

## 使用前后有什么变化

| 使用 Agent Switch 之前 | 使用 Agent Switch 之后 |
| --- | --- |
| 同一个 MCP 要分别配置到每个 Agent | 注册一次，再选择需要同步到哪些 Agent |
| API Key 被复制到多个原生客户端配置中 | 密钥值只保存在一个私密本地存储中 |
| MCP 修改后，不同客户端会悄悄产生漂移 | `doctor` 发现漂移，`reconcile` 修复漂移 |
| 每个 MCP 都可能继承完整的 Shell 环境 | 每个 Wrapper 只注入明确授权的密钥名称 |
| 迁移现有配置像一次不可控的大改动 | 先预览，再明确确认接管，并自动备份 |

新安装默认使用空注册表。Agent Switch 不会预装维护者喜欢的 MCP，也不会索取无关密钥。

## 核心工作流

```bash
# 1. 只预览支持的用户级 stdio MCP，不修改文件
agent-switch mcp import --dry-run --json

# 2. 备份原生配置，并明确接管预览过的 MCP
agent-switch mcp import --adopt

# 3. 通过本地管道写入密钥，不把值放进参数或聊天记录
secret-producing-command | agent-switch secret set --stdin SEARCH_API_KEY

# 4. 先检查健康状态与漂移，再应用中央配置
agent-switch doctor
agent-switch reconcile

# 5. 要求所有受管状态完全一致
agent-switch doctor --strict
```

不要在被记录的命令中用真实密钥替换 `secret-producing-command`。
macOS 应用是交互式录入密钥最简单的方式。

## Agent Switch 管理什么

| 领域 | 当前能力 |
| --- | --- |
| **MCP 服务** | 导入、新增、编辑、选择目标、启用、停用、删除并同步用户级 command/stdio MCP |
| **凭据** | 每个值只保存一次，每个 MCP 只能获得明确声明的密钥名称 |
| **Agent 策略** | 为 Codex、Claude Code 和 Hermes 同步有边界的指令块 |
| **健康检查** | 检测无效配置、缺失密钥名称、未固定版本的 `npx` 包、受阻目标和配置漂移 |
| **恢复** | 原子写入，并在迁移或替换原生文件前创建备份 |
| **CLI 清单** | 显示本机 AI 工具、版本、包管理器和可执行文件路径 |
| **Skills** | 读取可选的 Skill Hub 清单，但不会把“已下载”当成“已激活” |
| **CC Switch** | 保留 Provider 切换，只镜像 Agent Switch 自己拥有的 MCP 行 |

## 工作原理

```mermaid
flowchart LR
    UI["macOS 应用或 CLI"] --> C["中央配置"]
    C --> D["Doctor"]
    D --> R["Reconcile"]
    S["私密密钥存储"] --> W["每个 MCP 独立 Wrapper"]
    R --> X["不含密钥的 Agent 配置"]
    W --> M["MCP 进程"]
    X --> A["Codex"]
    X --> B["Claude Code / Desktop"]
    X --> H["Hermes"]
```

| 本地事实来源 | 默认路径 |
| --- | --- |
| MCP 定义与授权 | `~/.config/agent-switch/config.json` |
| 凭据值 | `~/.config/agent-switch/secrets.env` |
| 生成的 MCP Wrapper | `~/.config/agent-switch/mcp/bin/` |
| 原生配置备份 | `~/.config/agent-switch/backups/` |

Agent Switch 只管理 `agent-*` MCP 条目和带标记的指令块。其他 Provider 与 MCP
设置会被保留。

## 支持的集成

| 集成 | MCP 同步 | 共享策略 | 状态清单 |
| --- | :---: | :---: | :---: |
| Codex | ✓ | ✓ | ✓ |
| Claude Code | ✓ | ✓ | ✓ |
| Claude Desktop | ✓ | — | — |
| Hermes | ✓ | ✓ | ✓ |
| CC Switch | 仅镜像自己拥有的行 | 保留 Provider 设置 | Schema 检查 |
| Skill Hub | Skill 清单/更新 | 显式项目或全局 Profile | ✓ |

新增 Agent 必须先实现并测试专用适配器。Agent Switch 不会猜测未知配置格式。

## 凭据与隐私边界

Agent Switch 是本地优先工具，但它**不是密码保险库**。

- 凭据值保存在本地 mode-`0600` 文件中，不进入 Git、应用偏好、生成的
  Agent 配置或 Wrapper 源码。
- 写入密钥使用 stdin 或继承的文件描述符，不使用位置参数。
- 读取密钥时拒绝 stdout、stderr、终端和别名描述符。
- Wrapper 把密钥文件当数据解析，移除继承的敏感变量，只注入当前 MCP
  获得授权的名称。
- 诊断、导入预览和审计只报告密钥**名称**，不报告值。
- 缺少必需密钥名称时，Wrapper 会关闭失败。
- 产品没有上传服务或云账户；但 Agent 调用某个 MCP 时，该 MCP 仍可能访问自己的服务商。

使用敏感凭据前，请阅读[密钥与 Wrapper](docs/secrets-and-wrappers.md)和
[安全策略](SECURITY.md)。安全问题应按照 `SECURITY.md` 的私密渠道报告，不要创建公开 Issue。

## 当前 Alpha 范围

0.2 版本管理 macOS 上用户级 command/stdio MCP 定义和静态凭据值。

目前尚未提供：

- 原生 HTTP/SSE Transport 或 OAuth Session 迁移；
- 项目级 MCP 发现；
- 对未知 Agent 的自动支持；
- 经过 Developer ID 签名并公证的可下载应用；
- 已发布的 PyPI 包；
- 密码管理器或硬件保护的凭据保险库。

这些是明确限制，不是隐藏功能。计划中的工作记录在[路线图](docs/roadmap.md)中。

## 在 Claude Code / Codex 中使用

安装自带的 Skill，让 Agent 能安全地操作 `agent-switch` CLI（密钥只走 stdin，先预览再应用）：

```bash
npx skills add risingjerry/agent-switch
```

也可手动复制：`cp -R skills/agent-switch ~/.claude/skills/`（Claude Code）或 `~/.agents/skills/`（Codex 等兼容 Agent）。示例提示词：

- “运行 agent-switch doctor，告诉我哪些 MCP 有漂移或缺少密钥。”
- “把我各个 Agent 里已有的 MCP 导入进来，先给我看 dry run。”
- “通过 agent-switch 给 Codex 和 Claude Code 添加 filesystem MCP，然后 reconcile。”

要修改本仓库？Agent 请阅读 [AGENTS.md](AGENTS.md)。

## 常用命令

```bash
agent-switch agents               # 检测到的 Agent 与策略接入状态
agent-switch clis                 # 已安装的 AI CLI 清单
agent-switch mcp list             # 中央 MCP 注册表
agent-switch secret list          # 只显示凭据名称
agent-switch skills               # 可选的 Skill Hub 清单
agent-switch doctor --json        # 机器可读的健康报告
agent-switch reconcile --dry-run  # 计划执行的受管变更
```

新增一个中央管理的 MCP：

```bash
agent-switch mcp add filesystem \
  --command npx \
  --arg=-y \
  --arg=@modelcontextprotocol/server-filesystem@1.0.0 \
  --app codex \
  --app claude

agent-switch reconcile
```

完整生命周期命令和安全导入行为见[统一 MCP 注册表](docs/mcp-registry.md)。

## 更新

通过 Homebrew 安装的版本：

```bash
brew update
brew upgrade --cask risingjerry/tap/agent-switch
```

Agent Switch 不会静默更新自己、第三方 CLI 或 Skill 源。

## 常见问题

### 怎么让 Codex 和 Claude Code 共用同一个 MCP，而不用配置两遍？

用 `agent-switch mcp add` 注册一次，用 `--app` 选择目标应用，再运行 `agent-switch reconcile`。Agent Switch 会在每个 Agent 的原生配置里写入不含密钥的 `agent-*` 条目。

### MCP 的 API Key 应该存在哪里？

存进 Agent Switch 的私有存储，用 `agent-switch secret set --stdin NAME` 写入。值保存在 `~/.config/agent-switch/` 下权限为 0600 的文件里，不会出现在 Agent 配置、Git 或 shell 历史中。

### Agent Switch 是密码管理器吗？

不是。它是给 MCP Wrapper 用的本地优先凭据存储，注入时缺失即失败关闭；它不是密码库，也不依赖硬件保护。详见[密钥与 Wrapper](docs/secrets-and-wrappers.md)。

### 会覆盖我现有的 Codex 或 Claude 配置吗？

不会。它只管理 `agent-*` MCP 条目和带标记的指令块，修改原生文件前会备份；导入已有 MCP 必须先预览，再显式 `--adopt`。

### 支持 Windows 或 Linux 吗？

不支持。Agent Switch 仅支持 macOS 14+，包含原生 App 和 Python CLI。

### Homebrew Cask 为什么会移除 Gatekeeper quarantine？

Alpha 版 App 是临时（ad-hoc）签名，未经 Apple 公证。安装前请先审阅源码和发布校验值；公证已列入[路线图](docs/roadmap.md)。

### 能和 CC Switch 一起用吗？

可以。它保留 CC Switch 的供应商切换，只镜像自己拥有的 MCP 行。详见 [CC Switch 兼容性](docs/ccswitch-compat.md)。

## 卸载

先在 CLI 还在时清理受管状态：

```bash
agent-switch mcp list                  # 记下每个受管 MCP
agent-switch mcp remove NAME           # 逐个移除，同时清除其受管投影
agent-switch reconcile
```

再卸载软件：

```bash
brew uninstall --cask risingjerry/tap/agent-switch
brew list agent-switch-cli && brew uninstall agent-switch-cli   # 仅当 CLI formula 仍存在时
brew untap risingjerry/tap                                      # 可选
```

以下内容不会被自动删除（Agent Switch 不会擅自删你的数据）：

- `~/.config/agent-switch/`：包含注册表、`secrets.env`、Wrapper 和备份。确认不再需要其中的凭据后再删：`rm -rf ~/.config/agent-switch`。
- Agent 指令文件（如 `~/.claude/CLAUDE.md`）中带 `BEGIN AGENT SWITCH MANAGED INSTRUCTIONS` 标记的指令块，需手动删除。
- 你复制到 `~/.claude/skills/agent-switch` 或 `~/.agents/skills/agent-switch` 的 Skill。

## 文档

- [统一 MCP 注册表](docs/mcp-registry.md)
- [密钥与 Wrapper](docs/secrets-and-wrappers.md)
- [CC Switch 兼容性](docs/ccswitch-compat.md)
- [恢复与回滚](docs/recovery.md)
- [路线图](docs/roadmap.md)
- [参与贡献](CONTRIBUTING.md)
- [安全策略](SECURITY.md)

## 开发

需要 Python 3.11+、`pipx` 和完整 Xcode 的贡献者仍然可以使用源码安装器：

```bash
git clone https://github.com/risingjerry/agent-switch.git && cd agent-switch && ./scripts/install.sh
```

创建可编辑的 CLI 开发环境：

```bash
git clone https://github.com/risingjerry/agent-switch.git
cd agent-switch
python3 -m venv .venv
. .venv/bin/activate
python -m pip install -e .
python -m unittest discover -s tests
python -m unittest discover -s tests/integration
```

原生应用位于 [`macos-app/AgentSwitch`](macos-app/AgentSwitch)，最低支持
macOS 14。完整测试和隐私门禁见 [CONTRIBUTING.md](CONTRIBUTING.md)。

## 项目数据

| 公开指标 | 实时数据或最近一次维护者可见数据 |
| --- | ---: |
| Star / Fork / Commit / 发行下载量 | 见上方实时徽章 |
| README 访问量 | 见上方公开计数器；可能包含机器人和重复访问 |
| 仓库独立访客 | GitHub Traffic 最近 14 天滚动窗口内为 **0** |
| 独立克隆者 | 最近 14 天滚动窗口内为 **11**（共 **12** 次克隆） |

<sub>数据快照日期：2026-08-10。GitHub 只向仓库维护者提供克隆与独立访客数据，因此这里采用注明日期的透明快照，而不是需要私密 Token 的公开徽章。</sub>

### 克隆趋势

<p align="center">
  <a href="https://github.com/risingjerry/agent-switch">
    <img width="800" src="https://raw.githubusercontent.com/risingjerry/agent-switch/metrics/repository-metrics.svg" alt="Agent Switch 最近 14 天克隆趋势与仓库指标">
  </a>
</p>

<sub>曲线展示 GitHub Traffic 最近 14 天滚动窗口内的累计克隆次数。Traffic 序列采用注明日期的仓库所有者聚合快照；侧边的 Star、Fork 与 Commit 会在每次获得新 Star 后及每周自动刷新。README 中不嵌入任何长期 Token。</sub>

## 许可证

[MIT](LICENSE) © 2026 risingjerry

如果 Agent Switch 帮你省掉了复制粘贴 MCP 配置的麻烦，欢迎在 GitHub 点个 Star。

[![Star History Chart](https://api.star-history.com/svg?repos=risingjerry/agent-switch&type=Date)](https://star-history.com/#risingjerry/agent-switch&Date)
