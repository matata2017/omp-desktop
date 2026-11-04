# oh-my-pi-gui 复刻侦察（recon map）

目标：把 Electron 35 + React 19 的 `nornzach/oh-my-pi-gui`（v0.9.35，MIT）在 Rust/Tauri 2 上复刻成
`omp desktop`。复刻面是**功能与流程**：前端渲染层原样复用（MIT 允许），主进程用 Rust 重写。

分册（都在 `replica/parts/`，逐条带 `文件:行` 证据）：

| 分册 | 内容 | 规模 |
|---|---|---|
| `screens.md` | 屏与对话框清册 | 22 screens（S01–S22）+ 54 dialogs（D01–D54） |
| `flows.md` | 用户流程 + 状态机 | 26 flows（F01–F26）+ 17 个显式状态机 |
| `data-model.md` | 实体、持久化、RPC 命令面 | 28 实体（E01–E28）+ 197 条 RPC 命令 / 61 前缀 |
| `features.md` | 功能矩阵 | 100 行（must 23 / should 67 / could 10）+ 11 条 skip |
| `../features.csv` | 上表的 CSV 版（`feature,area,priority,original,clone,notes`） | 100 行 |

## 核心回路（F01）

`InputArea` 提交 → `planComposerSubmit` 决定 slash / steer / followUp → `rpc:command`
（`prompt`）→ sidecar 事件流 `message_update` / `tool_execution_*` / `agent_end` → `EventBatcher`
32 ms 合批 → 渲染层 `StreamingText` + 工具卡。**一条链路串起 104 个 IPC 方法里的一半**。

## 五条摘要

1. **核心回路**：composer 提交 → RPC `prompt` → 流式 markdown + 工具卡 → `agent_end` 结算；所有面板都是这条回路的旁挂观察器。
2. **规模**：22 屏 / 54 对话框 / 26 流程 / 28 实体 / 197 条 RPC 命令。渲染层 567 个文件、111,400 行非测试代码；主进程 83 个模块。
3. **三处最难**：
   - 内置浏览器（`browser-host.ts` 1,092 行）——原生 view 覆盖在 renderer 之上，需要 CDP 与离屏休眠策略；
   - 多 tab / 多窗口的 sidecar 归属（F-OWN）与 extension-UI 回包路由（F-UI-ORIGIN），一个 tab 一个 232 MB 进程；
   - 会话索引与会话文件格式（首行 256 B 标题槽 + 32 KiB 头尾扫描 + LRU 4096）。
4. **明确不复刻**（见 `features.md` 的 `## skip`）：Electron 构建管线、macOS 签名/公证、electron-updater 的 DMG 安装、GPU 回退、renderer 崩溃恢复、Playwright Electron E2E、macOS dock badge。沙箱/授权由 Tauri 的 capabilities 取代。
5. **下一步**：`crates/omp-core` + `crates/omp-rpc` + `crates/omp-agent` 已按 F01 落地并通过单测；协议门（6 条 fork 命令全 `success:true`）已实测通过。接下来是 Phase 2：Tauri 外壳 + `window.omp` shim。

## 复刻载体（`clone` 列含义）

| 值 | 含义 |
|---|---|
| `reuse` | 前端 TSX/TS 原样复用，Rust 只做 IPC/RPC 传输 |
| `rust` | 必须在 Rust 侧重写（进程、文件、数据库、WebView 控制） |
| `plugin` | 交给 Tauri 官方插件（dialog / opener / single-instance / window-state / updater / deep-link） |
| `hybrid` | 前后端各承担一半（例：argv 能力探测、会话索引） |

## 与主计划的对应关系

本侦察是 `local://omp-gui-rust-plan.md` 的输入而非替代：计划定**怎么建**（workspace 划分、
特性开关、打包），本册定**建什么**（功能、流程、实体、优先级）。两份冲突时以本册的
`replica/parts/*` 证据为准，计划里的验收标准不回退。
