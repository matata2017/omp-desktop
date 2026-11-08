# Romp

用 Rust 和 Tauri 2 复刻 [oh-my-pi-gui](https://github.com/nornzach/oh-my-pi-gui) v0.9.35 的桌面应用。渲染层沿用上游 React 代码，主进程、状态、会话、文件、搜索、终端、浏览器与进程管理用 Rust 重写。目标是降低常驻内存和冷启动时间，同时保留上游功能面。

当前只在 Windows x64 上验证。macOS 与 Linux 排在 1.0 之后。

## 状态

复刻仍在进行中。逐条功能状态见 [`docs/parity.md`](docs/parity.md)，机器可读源是 `replica/features.csv`（133 行）。上游侦察产物在 `replica/parts/`，含 22 个屏、54 个对话框、26 条流程、28 个实体与 197 条 RPC 命令。

已跑通的部分包括多 tab 与会话、流式回答与工具卡、模型与设置页、内置浏览器、终端面板、统计仪表盘、定时任务、托盘、`omp://` 深链、NSIS 安装包。每项的实际验证方式记在 [`docs/handoff.md`](docs/handoff.md)。

## 架构

```
src/renderer  上游 React 渲染层（原样复用，少量入口改动）
src/shared    上游类型与 rpc-client（合同的唯一来源）
src/bridge    window.omp shim，把 Electron preload 映射到 Tauri 的 ipc_invoke
src-tauri     Tauri 外壳：窗口、托盘、菜单、深链、单一 IPC 入口
crates/       常驻 Rust 服务：omp-agent / omp-core / omp-rpc /
              omp-session / omp-stats / omp-scheduler
resources/    内置 omp sidecar（omp.exe，被 .gitignore 排除）
```

一个活动 tab 对应一个 `omp --mode rpc-ui` 子进程，约 231 MB。共享单个 sidecar 服务多个 tab 的做法已用 `scripts/perf/probe-shared.mjs` 否决：切换会话会让在途回答脱离观察者，事件帧不带会话归属。常驻进程上限由 `ROMP_MAX_SIDECARS` 控制（默认 3），超出时按 LRU 回收空闲 tab。设计决定见 [`docs/architecture.md`](docs/architecture.md)。

## 构建与运行

前置条件：Windows 10/11、Rust 1.90（edition 2024）、[bun](https://bun.sh)、Visual Studio Build Tools（含 VC.Tools 与 Windows 10 SDK）、WebView2 Runtime（Windows 11 自带）。`resources/omp.exe` 需要单独放入，它不入库。

```bash
bun install
```

开发模式用脚本启动，它会注入 MSVC 环境、`OMP_SIDECAR` 和 CDP 端口：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dev.ps1
```

发布版必须用 `tauri build`，不能用 `cargo build --release`。后者仍指向 `devUrl`（localhost:1420），Vite 一停窗口就白屏。

```bash
# 只出 release exe
bunx tauri build --no-bundle

# 出 NSIS 安装包，sidecar 随包安装
bunx tauri build
# target/release/bundle/nsis/Romp_0.1.0_x64-setup.exe
```

安装包为 currentUser 模式，默认装到 `%LOCALAPPDATA%\Romp`，卸载入口是同目录的 `uninstall.exe`。安装会写 HKCU 的 `omp://` 协议关联。

## 配置

| 变量 | 作用 |
|---|---|
| `OMP_SIDECAR` | sidecar 路径。优先级最高，开发时默认 `resources/omp.exe` |
| `OMP_DATA_DIR` | 数据目录，默认 `%APPDATA%\dev.romp.desktop` |
| `ROMP_MAX_SIDECARS` | 常驻 sidecar 上限，默认 3 |
| `ROMP_STOCK_SIDECAR` | 设置后按上游 sidecar 的能力探测，不启用 fork 专属参数 |
| `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` | 传给 WebView2，例如 `--remote-debugging-port=9222` |

## 性能

发布版实测（Windows x64，进程树口径）：

| 状态 | 空闲常驻 |
|---|---:|
| 1 tab | 712.0 MB |
| 2 tab | 978.7 MB |
| 4 tab | 1442.7 MB |

固定开销约 480 MB，每多一个 tab 约 240 MB。冷启到窗口 275 到 694 ms，首帧 1.7 到 2.8 s，首会话可交互约 4 s。`omp stats` 是懒启动，空闲不占内存。方法、方差、上游 Electron 对照与预算门见 [`docs/perf.md`](docs/perf.md)。

## 测试与工具

```bash
cargo test --workspace     # 158 passed（16 suites）
bun run typecheck          # tsc --noEmit
bun run test               # 1571 passed（186 files）
bun run perf:gate          # 冷启与内存回归门，基线在 docs/perf-baseline.json
bun run ui:audit           # 文案扫描 + axe + 截图，需要运行中的窗口与 9222
```

`scripts/perf/` 是唯一的测量器：`measure.ps1` 测冷启与进程树内存，`drive.mjs` 通过 CDP 开 tab 并读启动标记，`gate.ps1` 判回归，`profile-stream.mjs` 录流式回答，`probe-shared.mjs` 验证进程模型。`scripts/ui/audit.mjs` 扫 locale 文案并注入 axe-core。`scripts/cdp-eval.mjs` 可在运行中的窗口执行表达式。

## 文档

| 文件 | 内容 |
|---|---|
| [`docs/architecture.md`](docs/architecture.md) | 分层、进程生命周期、决策记录 |
| [`docs/parity.md`](docs/parity.md) | 与上游的逐条功能账本 |
| [`docs/perf.md`](docs/perf.md) | 发布版基线、方法、已知问题 |
| [`docs/handoff.md`](docs/handoff.md) | 验证记录、构建配方、许可清单 |
| [`replica/recon.md`](replica/recon.md) | 上游侦察地图 |
| [`CHANGELOG.md`](CHANGELOG.md) | 上游变更记录 |

## 许可

本仓库同时承载三段来源不同的代码，许可各不相同，完整清单见 [`NOTICE`](NOTICE)：

- 本工程原创代码（`crates/`、`src-tauri/`、`src/bridge/`、`scripts/`、`docs/`、`replica/`）：MIT，见 [`LICENSE`](LICENSE)。
- 渲染层 `src/renderer/`、`src/shared/`：来自 `nornzach/oh-my-pi-gui` v0.9.35，MIT，见 [`LICENSES/oh-my-pi-gui-MIT.txt`](LICENSES/oh-my-pi-gui-MIT.txt)。相对上游的字节级改动清单在 `docs/handoff.md` 第 9 节。
- 工作流模板 `.trellis/`、`.omp/skills/trellis-*`：来自 `mindfold-ai/Trellis`，AGPL-3.0，见 [`LICENSES/trellis-AGPL-3.0.txt`](LICENSES/trellis-AGPL-3.0.txt)。

发布或再分发时，三段声明需要一起随附。
