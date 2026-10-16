<div align="center">

<h1>gr0k(<code>gr0k</code>)</h1>

<img src="docs/assets/hyper-banner.jpg" alt="gr0k —— 终端 AI 编码代理" width="720">

<p>
  <a href="https://github.com/opensamai/gr0k/releases"><img src="https://img.shields.io/github/v/release/opensamai/gr0k?display_name=tag" alt="发布版本"></a>
  <a href="https://github.com/opensamai/gr0k/actions/workflows/release.yml"><img src="https://github.com/opensamai/gr0k/actions/workflows/release.yml/badge.svg" alt="发布 CI"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-blue" alt="许可证"></a>
  <img src="https://img.shields.io/badge/rust-1.94.0-orange?logo=rust" alt="Rust 1.94">
  <img src="https://img.shields.io/badge/platform-macOS%20%C2%B7%20Linux%20%C2%B7%20Windows-lightgrey" alt="平台:macOS、Linux、Windows">
  <a href="https://github.com/opensamai/gr0k/releases"><img src="https://img.shields.io/github/downloads/opensamai/gr0k/total?label=downloads" alt="下载量"></a>
  <img src="https://img.shields.io/badge/i18n-10%20locales-brightgreen" alt="i18n:10 种语言">
</p>

**gr0k** 是 [Grok Build](https://github.com/xai-org/grok-build) 的非官方多供应商社区构建版本 ——
一个用 Rust 编写的终端 AI 编码代理,对多家 LLM 供应商提供一流支持:
xAI Grok、Kimi Code / Moonshot、ChatGPT Codex、OpenAI、Anthropic、Z.AI、Ollama Cloud 等。

它以全屏 TUI 的形式运行,能够理解你的代码库、编辑文件、
执行 shell 命令、搜索网页,并管理长时间运行的任务 ——
既可以在终端中交互使用,也可以无头模式用于脚本/CI,
还能通过 Agent Client Protocol(ACP)嵌入到编辑器中。
UI 已本地化为 10 种语言（English、简体中文、繁體中文、日本語、한국어、
Español、Português、Français、Deutsch、Русский），并可在设置中实时切换。运行
`gr0k dashboard --web` 可以打开本地只读的纯 Rust Web 仪表盘，查看会话指标、
事件时间线、图表、日志和实时事件流。

[安装](#安装) ·
[供应商](#供应商) ·
[从源码构建](#从源码构建) ·
[发布](#发布) ·
[与官方 <code>grok</code> 共存](#与官方-grok-共存) ·
[许可证](#许可证)

**English: [README.md](README.md)** ·
**中文用户指南: [docs/user-guide-zh-CN/](crates/codegen/xai-grok-pager/docs/user-guide-zh-CN/)**

</div>

---

## 界面截图

真实 TUI(在 PTY 中用仓库内置的
[`tui_shot`](crates/codegen/xai-grok-pager-pty-harness/examples/tui_shot.rs)
工具捕获),展示 10 种 UI 语言中的两种:

| English | 简体中文 |
| ------- | -------- |
| ![gr0k TUI in English](docs/assets/screenshot-welcome-en.png) | ![中文界面的 gr0k TUI](docs/assets/screenshot-welcome-zh.png) |

---

## 为什么叫 “gr0k”？

这个社区构建以前以 `hyper` 发布。产品名现在是 **gr0k**，仓库是
[opensamai/gr0k](https://github.com/opensamai/gr0k)。合入官方 Grok Build
并保留多供应商补丁的代码在 `merge-official` 分支。`main` 仍是更早的单平台快照。

| | 官方版本 | 本构建 |
|---|---|---|
| 产品 | Grok Build | **gr0k** |
| 二进制文件 | `grok` | **`gr0k`** |
| 安装目录 | `~/.grok` | **`~/.gr0k`**（仅二进制） |
| 配置 / 认证 / 会话 | `~/.grok` | **`~/.grok`**（同一运行时） |
| 上游 | [xai-org/grok-build](https://github.com/xai-org/grok-build) | `merge-official` |

`GR0K_SHARE_DIR` 可改安装根目录。旧变量 `HYPER_SHARE_DIR` 仍然有效。
以前装在 `~/.hyper/bin/hyper` 的文件不会被自动搬走，新安装使用
`~/.gr0k/bin/gr0k`。

---

## 安装

安装脚本会下载发布包，用 `SHA256SUMS` 校验，再链到
`~/.gr0k/bin/gr0k`（Windows 为 `%USERPROFILE%\.gr0k\bin\gr0k.exe`）。

```sh
# macOS / Linux —— 脚本在合并分支上
curl -fsSL https://raw.githubusercontent.com/opensamai/gr0k/merge-official/install.sh | bash
```

```powershell
# Windows PowerShell
irm https://raw.githubusercontent.com/opensamai/gr0k/merge-official/install.ps1 | iex
```

```sh
gr0k --version
gr0k                # 启动 TUI
```

当前已发布的 `v0.2.115` 是更早的快照：只有一个仍叫
`hyper-0.2.115-…` 的 macOS arm64 包。合并后的代码版本号相同，但二进制名是
`gr0k`，目标是五个平台（macOS arm64/x86_64、Linux glibc arm64/x86_64、
Windows x86_64）。这些包由
[`.github/workflows/release.yml`](.github/workflows/release.yml) 生成，
**在推送新 tag 之前不会挂到 Release 上**。在那之前，请按下面的方式从源码安装。
手动触发 workflow 只上传 Actions 产物，不会发布 Release。

> 未签名的 macOS 二进制从浏览器下载后，Gatekeeper 仍会警告。`install.sh`
> 安装后会清掉隔离属性。签名和公证需要 Apple Developer ID，CI 里没有这一步。

---

## 供应商

gr0k 保留了本代码树中的多供应商注册表(见 pager
[用户指南（中文）](crates/codegen/xai-grok-pager/docs/user-guide-zh-CN/)):

| 平台 | 认证方式 | 备注 |
| -------- | ---- | ----- |
| xAI / Grok | `gr0k login`（OIDC）或 `XAI_API_KEY` | 第一方模型 |
| Kimi Code | `gr0k login --kimi` 或 `/kimi` | 设备 OAuth；`kimi-code/*` |
| Moonshot CN / AI | API key | 开放平台 |
| ChatGPT Codex | `gr0k login --openai` 或 `/codex` | ChatGPT OAuth；实验性 `/live` 语音 |
| **Anthropic Claude（Pro/Max）** | **`gr0k login --claude`** 或 **`/claude`** | 订阅登录；`anthropic-claude/*` |
| **Nexus 中转站** | **`/nexus <key> [base_url]`**(TUI)或 `NEXUS_API_KEY` | 自托管、兼容 OpenAI/Anthropic 的中转网关;`nexus/*` |
| OpenAI / Anthropic / DeepSeek 风格 | API keys | BYOK 目录 |
| Z.AI Coding Plan | 平台 key | 国际版方案 |
| Ollama Cloud | API key | 实时模型清单同步 |

选择器中的模型 id 形如 `{platform}/{model}`(例如
`kimi-code/k3`、`codex:gpt-5.5`)。各平台文档位于
`crates/codegen/xai-grok-pager/docs/user-guide-zh-CN/`(Moonshot、Kimi Code、
OpenAI Codex 等)。

配置和凭据仍然存放在 **`~/.grok`**(与上游 Grok Build 相同的路径),
因此已有的会话、API key 和 `auth.json` 可以继续使用。

Kimi、Codex 或 Claude 返回 401 时，只刷新**该订阅**的令牌，不会改去刷新
xAI 会话。Ollama 一类开放平台的 BYOK 端点同样不会靠刷新 xAI 来修复。
`gr0k logout --all` 会清掉 xAI 会话以及 Kimi、Codex、Claude 的 OAuth
范围，不会删除 BYOK 平台密钥。

```sh
gr0k login                  # xAI / Grok
gr0k login --kimi           # Kimi Code
gr0k login --openai         # ChatGPT Codex（别名：--chatgpt）
gr0k login --claude         # Claude Pro/Max（别名：--anthropic）
gr0k logout --kimi          # 只清一个范围
gr0k logout --openai
gr0k logout --claude
gr0k logout --all           # xAI + 上述三个订阅
```

### 主题

`/theme` 除上游主题外，还有 gr0k 预设：Everforest、Nord、Dracula、Gruvbox、
Catppuccin Mocha/Latte、Solarized 暗色/亮色、Deep Ocean、Ember、Midnight OLED
和 Paper。`terminal` 保留终端自己的背景。`auto` 跟随系统外观。预设编号
`6` 到 `17` 是磁盘上的稳定值，不要改号。

### Anthropic Claude 号登录（Pro/Max）

使用 Anthropic Claude 的 **Pro/Max 订阅账号**登录,即可直接使用
`anthropic-claude/*` 模型 —— 无需 API key。认证走浏览器 OAuth 流程
(PKCE + 本地回环回调;无头 / 远程机器可手动粘贴授权码)。它与 xAI
登录 **相互独立**,因此可以同时登录 Grok 和 Claude。

```sh
# TUI(最简):一条命令即可,登录后凭据长期有效
/claude                     # 等价于 /login claude

# CLI:打开浏览器 OAuth 流程
gr0k login --claude        # 别名:gr0k login --anthropic

# 退出登录
gr0k logout --claude       # 或:gr0k logout --all
```

凭据存储在 `~/.grok/auth.json` 的 `oauth/anthropic-claude` 作用域下,
token 会自动刷新。登录后在模型选择器中挑选任意 `anthropic-claude/*` 条目即可。

### Nexus 中转站(relay gateway)

**Nexus** 是一个自托管、兼容 OpenAI/Anthropic 的**中转网关**
(BYOK,Bearer API key)。一个 key 即可解锁它代理的全部三种后端 ——
OpenAI 的 `chat/completions`、Claude **Messages** 以及 **Responses** API,
模型清单会从网关的 `/models` 实时发现。

网关根地址默认为 `https://nexuscore.now`。由于 Nexus 是自托管的,
你可以通过三种方式指向自己的部署:

```sh
# 0. 命令行(登录 xAI 之前就能用)—— 全局一次性配置
gr0k nexus <api_key>                   # 保存 key 到 ~/.grok/auth.json
gr0k nexus <api_key> https://your-gw   # 保存 key + 自定义网关根地址
gr0k nexus --clear                     # 删除已保存的 key
gr0k nexus                             # 打印引导 + 当前状态

# 1. TUI(最简)—— key 只需输一次,自动持久化到 ~/.grok/auth.json
/nexus                                 # 打印引导(去哪申请 key、怎么用)
/nexus <api_key>                       # 使用默认根地址
/nexus <api_key> https://your-gateway  # 同时持久化自定义网关根地址
/nexus clear                           # 删除已保存的 key(别名:off / logout)
#    以上等价于 /providers nexus …,只是更短

# 2. 环境变量
export NEXUS_API_KEY=sk-nexus-…       # 或 GROK_NEXUS_API_KEY
export GROK_NEXUS_BASE_URL=https://your-gateway   # 覆盖根地址

# 3. 两种方式配置后,模型都会以 nexus/* 出现在选择器中
```

base URL 可以是任意面向客户端的形态(`https://your-gateway`、
`…/v1`、`…/openai/v1`)—— gr0k 会将其规范化为裸根地址,并自动推导出
各后端路径(`{root}/openai/v1`、`{root}/v1`)。key 和根地址会与其他
供应商一起保存到 `~/.grok/auth.json`。

---

## 从源码构建

环境要求:

- **Rust** —— 由 [`rust-toolchain.toml`](rust-toolchain.toml) 锁定版本
  (首次构建时 `rustup` 会自动安装)
- **[DotSlash](https://dotslash-cli.com)** —— 密封的 `bin/protoc`
  ```sh
  cargo install dotslash
  # 或者:brew install dotslash
  ```
- **CMake 3.5+** —— 构建实验性 `/live` 语音使用的内置静态 Opus；工作区已固定
  `CMAKE_POLICY_VERSION_MINIMUM=3.5`

```sh
cargo run -p xai-grok-pager-bin              # 构建并启动 TUI(二进制名:gr0k)
cargo build -p xai-grok-pager-bin --profile release-dist
./target/release-dist/gr0k --version
```

组合根包仍然是 `xai-grok-pager-bin`(monorepo 布局);
**发布的二进制名称**是 `gr0k`。

### 将源码构建全局安装

如果想把你自己的改动作为托管的 `~/.gr0k/bin/gr0k` 运行(与 `install.sh`
生成的目录布局一致,但从当前检出的源码构建,而非 GitHub 发布版),
可使用 [`scripts/install-local.sh`](scripts/install-local.sh):

```sh
scripts/install-local.sh                 # 构建并安装,覆盖 ~/.gr0k/bin/gr0k
scripts/install-local.sh --no-link       # 构建并暂存到 downloads/,保留当前软链接
GR0K_SHARE_DIR=~/.gr0k-dev scripts/install-local.sh   # 安装到独立目录
```

脚本会执行 `cargo install --locked`,在 `~/.gr0k/downloads/` 下暂存带版本号
的二进制,在 macOS 上重新签名,冒烟测试 `--version`,再原子地将
`~/.gr0k/bin/gr0k` 指向新版本。需要 `cargo`、`cmake` 以及真实的 `ripgrep`
二进制(自动探测,或设置 `GROK_SHELL_BUNDLE_RG_PATH`)。首次构建约需 13 分钟。

---

## 更新日志

发布说明见 [`CHANGELOG.md`](./CHANGELOG.md)。已知限制见:
[`docs/KNOWN_ISSUES.md`](./docs/KNOWN_ISSUES.md)。

---

## 发布

1. 将根目录的 [`VERSION`](VERSION) 文件设置为 **monorepo 锁步客户端版本**
   (与 `crates/codegen/xai-grok-pager/Cargo.toml` /
   `xai-grok-version` 保持一致，当前为 `0.2.115`）。CI 会把它编译进
   `x-grok-client-version`；xAI 会拒绝低于 **0.1.202** 的客户端（HTTP 426）。
   **不要**自己编造一个较低的营销版本号（例如 `0.1.0`）。
2. 在 `merge-official`（或你的发布分支）上提交；更新 `CHANGELOG.md`。
3. 打标签并推送 —— CI 会构建五个目标并发布 GitHub Release。
   `workflow_dispatch` 编同样的五个目标，但只上传 Actions 产物。
   Windows 目标在 Rust 1.94 上编译 `cc` 1.2.48 会失败
   （`FILE_ATTRIBUTE_TEMPORARY` 在该版本是 `i32`）；macOS 和 Linux
   不会编译这段代码。

```sh
VERSION=$(tr -d '[:space:]' < VERSION)
git tag "v${VERSION}"
git push origin "v${VERSION}"
```

工作流:[`.github/workflows/release.yml`](.github/workflows/release.yml)

构建产物:

| 产物 | 示例 |
| ----- | ------- |
| macOS arm64 | `gr0k-0.2.115-aarch64-apple-darwin.tar.gz` |
| macOS x86_64 | `gr0k-0.2.115-x86_64-apple-darwin.tar.gz` |
| Linux x86_64（glibc） | `gr0k-0.2.115-x86_64-unknown-linux-gnu.tar.gz` |
| Linux arm64（glibc） | `gr0k-0.2.115-aarch64-unknown-linux-gnu.tar.gz` |
| Windows x86_64 | `gr0k-0.2.115-x86_64-pc-windows-msvc.zip` |
| 校验和 | `SHA256SUMS` |

标签必须与 `VERSION` 完全一致（`v0.2.115` ↔ `0.2.115`），否则构建会失败。

---

## 与官方 `grok` 共存

gr0k 与 xAI / SpaceXAI **没有隶属关系**。在同一台机器上:

| 项目 | 官方 `grok` | gr0k |
|---------|-----------------|-------|
| 二进制 | `grok` | `gr0k` |
| 托管安装目录 | `~/.grok/bin` | `~/.gr0k/bin`(旧版 `~/.hyper/bin`) |
| 配置 / 认证 / 会话 | `~/.grok` | **相同的** `~/.grok` |
| Leader IPC(`leader*.sock` / `.lock`) | 位于 `~/.grok` | **相同的**命名空间 |

注意事项:

- 会话、API key 和 OAuth 权限是共享的 —— 登录一次,两个 CLI 都能看到。
- Leader 的 list/kill 可以同时看到两个产品的 leader。请只 kill 你自己启动的 leader。
- 社区构建版禁用了上游的自更新功能,因此 `gr0k update` 不会覆盖 `~/.grok/bin/grok`。升级 gr0k 请重新运行 `install.sh` / `install.ps1`。

gr0k 的安装脚本不会改写官方安装程序的任何内容。

---

## 构建说明(本 fork)

```sh
# 默认启用 community-build(gr0k 品牌 + 禁用上游更新器)。
cargo run -p xai-grok-pager-bin

# 显式构建发布风格的本地二进制
cargo build -p xai-grok-pager-bin --profile release-dist --features community-build
```

Amp 风格的 **agent 模式**(low / medium / high / ultra 档位)目前**仅有设计文档** ——
见 [`docs/design-modes.md`](docs/design-modes.md),尚未发布。

已知问题和剩余工作:[`docs/KNOWN_ISSUES.md`](docs/KNOWN_ISSUES.md)。

---

## 文档

仓库内用户指南(示例中可能仍写着 `grok`;gr0k 的二进制名是
`gr0k`,路径仍在 `~/.grok` 下):

- 中文：[用户指南（中文）](crates/codegen/xai-grok-pager/docs/user-guide-zh-CN/)
- English：[User Guide](crates/codegen/xai-grok-pager/docs/user-guide/)

相关扩展文档：

- [Hooks 与 Plugins 指南（中文）](crates/codegen/xai-grok-pager/docs/hooks-and-plugins.zh-CN.md)
- [自定义 Hooks 指南（中文）](crates/codegen/xai-grok-pager/docs/custom-hooks.zh-CN.md)

上游产品文档:[docs.x.ai/build](https://docs.x.ai/build/overview)

`SOURCE_REV` 记录了本代码树最近一次同步的 monorepo 提交。

---

## 仓库结构

| 路径 | 内容 |
|------|----------|
| `crates/codegen/xai-grok-pager-bin` | 组合根;构建 `gr0k` 二进制 |
| `crates/codegen/xai-grok-pager` | TUI |
| `crates/codegen/xai-grok-shell` | Agent 运行时 |
| `install.sh` / `install.ps1` | 发布安装脚本 |
| `.github/workflows/release.yml` | 多平台发布 CI |

> [!IMPORTANT]
> 根目录的 `Cargo.toml`(workspace 成员 / 依赖版本)是从 monorepo
> **生成**的 —— 请将其视为只读。本地修改请编辑各个 crate 自己的
> `Cargo.toml`,这样在同步时才能保留下来。

---

## 许可证

Apache-2.0。见 [`LICENSE`](LICENSE)、[`NOTICE`](NOTICE) 和
[`THIRD-PARTY-NOTICES`](THIRD-PARTY-NOTICES)。

基于 Grok Build 开源项目
([xai-org/grok-build](https://github.com/xai-org/grok-build))。
