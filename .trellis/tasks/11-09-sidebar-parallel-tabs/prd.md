# PRD: 侧边栏会话并行打开

## Goal

让两个会话能同时跑：侧边栏点击另一会话时，若当前 tab 已承载会话，新开独立 tab/sidecar，而不是把当前 tab 原地切走。

## Background（已核实事实）

- `src/renderer/hooks/use-session-switch.ts:282-291` `openSessionFromSidebar`：当前 tab 满足 `canBrowseInPlace`（`:247-274`）时走 `switchSessionNow` 原地切，否则 `tabs.openTab` 新开。
- 文件头注释 `:11-12` 与 `docs/handoff.md:71-72`：`switch_session` 会 abort 在途回合；`docs/architecture.md:146` 与 `docs/handoff.md:348` 定性为「串行复用，不是并发共享」。共享单 sidecar 已于 11-06 / 11-09 用 `scripts/perf/probe-shared.mjs` 否决（切走后在途 `message_update` / `agent_end` 不可恢复）。
- **判据选 `useMessagesStore.getState()` 的 hydrated transcript + `liveMessages`**，辅以 `tab.sessionPath`。被排除的信号各有实证缺陷：
  - `tab.sessionPath` 单独用：`tab:get-all`（`ipc.rs:122-129`）、`emit_tab_status`（`lib.rs:123-130`）都不发该字段，`handleSessionInfo`（`use-rpc-events.ts:835-838`）也不写；renderer 唯一来源是 `openTab` 本地插入（`tabs.ts:622`）。**重启恢复的 tab 该字段为 undefined** —— 正是用户截图场景。
  - 让 Rust 补发：`sync_session_binding`（`ipc.rs:1197-1209`）在每次 `get_state` 时给**全新 tab** 也写 `sessionFile`（注释明说 "including for a fresh tab's first session"），会污染空白判定。
  - `messageCount > 0`：新 session 的 `get_state` 返回 `messageCount:1`（本会话 rpc-caps 探针实测）。
  - `sessionRuntimeStore(tab.id,"session").sessionFile`：同样被 `get_state` 污染，且本测试 harness 无 runtime、该调用恒返回 null（`session-runtime-context.tsx:97-99`）。
  - `useMessagesStore.getState()` 是 scoped hook，`getState()` 跟随 `focusedSessionRuntime()`（`session-runtime-context.tsx:182`），而 `openSessionFromSidebar` 的 tab 正是从 `focusedSessionRuntime()?.tabId ?? tabs.activeTabId` 取的 —— 天然同一个 tab。生产里 `createTabRuntime` 为每个 tab 建 "messages" store、`hydrateTabSession` 往里写；测试里无 runtime 回落 default store。上游 `TabBar.tsx:478-487` 复合空白谓词亦按 tab 取 scoped store。
- **F-OWN 是硬约束**：`switchSessionNow:158-159` 命中 owner → `switchTab`；`openTab` 在 `tabs.ts:575-579` 遇 `refusal:"owned"` 也只 `switchTab`；`ipc.rs:154-157` 直接不 spawn。所以「切到已有 owner tab」不等于「原地 `switch_session`」。**不同会话本来就能并行**：本次 `runtime.log` 有 3 tab / 3 sidecar。
- 并行能力已存在：每 tab 一个 sidecar（`ipc.rs:134-181`），上限 `ROMP_MAX_SIDECARS=6`（`:1485`），`is_evictable`（`:1475`）保护 active / streaming；上限满且无 victim 时 `ipc.rs:138-139` 静默返回 `Value::Null`。
- `openTab` 的 spawn 载荷只有 `{cwd, sessionPath, kind, worktree}`（`tabs.ts:550-556`）；`browsing` 是 renderer store 本地标记（`:598` / `:624`），**不进 RPC 载荷**。
- `openTab` 会设 `pendingSessionPath`，ready 后 `tabs.ts:1072-1073` 在 `currentFile !== sessionPath` 时合法补发 `switch_session`；`ipc.rs` 只 debug 打印 channel 名，`runtime.log` 从不记录命令类型。故「日志无 `switch_session`」既不可观测也不成立。
- `src/renderer/hooks/use-session-switch.test.ts` 原 `:508-529` 把「有历史的 tab 连续原地切 a→b→c」钉成契约，属既有实现测试，必须改写。
- 用户运行的是 `%LOCALAPPDATA%\Romp` 的安装版；`dist` 是打进安装目录的资源，改 renderer TS 后必须重建 NSIS 并静默安装才会生效。

## Requirements

- R1 侧边栏点击会话时，若当前 tab 已承载会话（hydrated transcript 历史、`liveMessages` 或已绑定 `sessionPath`），一律走 `openTab` 新开 tab，不调用 `switchSessionNow`。
- R2 真正空白的 tab（三者皆空；以 `TabBar.tsx:478-487` 复合谓词为语义交叉校验）保持原地切，避免刚启动时为浏览历史多开进程。
- R3 保留 `canBrowseInPlace` 的全部现有守卫（kind、workspace、streaming、compacting、草稿、审批、队列、extension UI、pendingSessionPath、worktree）。
- R4 不改 `requestSessionSwitch` / `SessionSwitchDialog` / `switchSessionNow` 本身；⌘P、deep link、对话框路径维持原语义。
- R5 不改 Rust 侧 `tab:spawn`、`confirmed_switch_path`、`tab_sessions`、pool claim/release 与 sidecar 协议。
- R6 同步更新所有描述「同 workspace 一律原地切」的文档，并保留「协议层共享 sidecar 仍被否决」这一结论。
- R7 测试 seed **不得伪造 `sessionPath`**：改为给 `useMessagesStore` 灌历史，并断言恢复态 tab 的 `sessionPath === undefined`。

## Acceptance Criteria

- A1 单元测试：已承载会话的 ready tab 点同 workspace 另一会话 → `tabs.spawn` 收到含 `sessionPath` 的载荷（**不含 `browsing`**），`rpc.switchSession` 不被调用；store 中的新 tab 带 `browsing: true`。
- A2 单元测试：空白 tab 点会话 → `switchSession` 被调用、`tabs.spawn` 不被调用。
- A3 单元测试：现有全部 `never switches ...` 用例继续通过。
- A4 单元测试：**恢复态用例**（tab 无 `sessionPath`、但 `useMessagesStore` 有历史）仍新开 tab；连续点两个不同会话 → `tabs.spawn` 2 次、store 含 2 个 tab、各自 `sessionPath` 正确；仅 `liveMessages` 非空也新开 tab。
- A5 门：`bunx vitest run src/renderer/hooks/use-session-switch.test.ts` 全绿；`bun run test` 全绿；`bun run typecheck` 0 错误；`cargo test --workspace` 无回归。
- A6 真实并行冒烟：A 起 prompt → 侧栏点 B → **A 的流继续到 `agent_end`**、B 正常 ready。可观测信号为 `tab:spawn` ×2 / `spawning sidecar` ×2 / `omp.exe` 进程 ×2；不得用「日志无 `switch_session`」当不变量。
- A7 文档 8 处（见 design.md §6）更新完毕，无残留过时表述；`docs/handoff.md:124` 分叉清单仍只登记 `use-session-switch.ts`。
- A8 **交付到安装版**：`bunx tauri build` 重建 `dist` + NSIS → 静默安装 → 在安装版重放 A6。只改源码 / 只跑单测不算完成。

## Out of Scope

- 不做协议层共享 sidecar（已被 `probe-shared.mjs` 否决，需改 upstream fork）。
- 不改 `ROMP_MAX_SIDECARS` 默认值、不做 warm spare、不改 LRU 回收策略。
- 不改侧边栏 UI、不加「在新标签打开」菜单项、不加设置项。
- **不改 `requestSessionSwitch` 的 idle 分支**：⌘P / deep link / SessionSwitchDialog 仍原地切，那些入口仍可能复现「两个不能同时跑」；本任务只管侧边栏点击。
- 不解决 advisor TUI 渲染错乱（`tui_open` / `terminal_run_cli` 为 `docs/parity.md:226` 的 todo），另案处理。
- 不新增 renderer 分叉文件。

## Constraints

- renderer 沿用上游，字节级改动仅允许 `main.tsx`、`index.html`、`ChatStream.tsx`、`use-session-switch.ts`（`docs/handoff.md:124`）。本次仍落在 `use-session-switch.ts` 内。
- 每个并行会话多一个约 500 MB 的 agent 进程；上限 6。`is_evictable` 要求 `has_session`，未绑定会话的空 tab 不可驱逐；满时 `tab:spawn` 静默失败。
- Windows x64 优先；新 tab 首次点击需 spawn + resume（约 0.9–2.5 s），由现有 `starting` 状态与骨架承接。
- 用户当前运行安装版且有 3 个 tab / 在途会话；重装前需正常退出，**不强杀**（`docs/handoff.md:252`：强杀主进程会留下 sidecar 孤儿）。

## Risks

- 进程数 / 内存上升，6 上限更易触发 → 保持 `ROMP_MAX_SIDECARS` 可调，本任务不调默认值。
- 全新 session 的 transcript 是否含 context 行未单独验证；若含，空白 tab 也会新开进程 —— 功能仍正确，只是省进程的优化失效。
- 文档漂移：多处把原地切写成既定架构，必须同步；但不得连带删除共享 sidecar 被否决的实验结论。
