# 现状调研: 侧边栏会话切换与并行边界

> `implement.jsonl` / `check.jsonl` 的注入材料。原始 `docs/handoff.md`（36.6 KB）与
> `docs/parity.md`（38.5 KB）超过 32 KB 注入上限会被截断，故把相关事实抽到此处。

## 1. 调用链（已核实）

```
Sidebar.tsx:588-589  onClick → openSessionFromSidebar(session)
use-session-switch.ts:282-291
  ├─ canBrowseInPlace(tab, session) === true  → switchSessionNow(session) + setBrowsing(tab.id, true)
  └─ false → tabs.openTab({ cwd, sessionPath, kind, browsing: true })
tabs.ts:542-630  openTab → window.omp.tabs.spawn(...) → ipc.rs:134-181 tab:spawn
```

`canBrowseInPlace`（`use-session-switch.ts:247-274`）现有守卫，逐条：

1. kind 不匹配 / `tab.worktree` / `tab.pendingSessionPath` → false
2. `tab.status !== "ready"` / `state.isStreaming` / `state.isCompacting` / `state.switchPending` → false
3. `session.cwd !== tab.cwd` → false（注释说明必须用 `tab.cwd`，`state.cwd` 会漂移）
4. **新增**：`tab.sessionPath || useMessagesStore.getState().messages.length > 0 || liveMessages.length > 0` → false
5. `composer.sending` / 草稿非空 / 有图片 → false
6. `extensionUi.pendingRequests` / `planApproval.pending` / `queue.steering|followUp` → false
7. 其余 → true

`canBrowseInPlace` 全仓库只在 `:286` 一处被调用，改动收敛到单文件。

## 2. 为什么「不能两个同时跑」

- `use-session-switch.ts:11-12` 文件头注释：`switch_session` 会 abort 在途回合（TUI parity）。
- `docs/architecture.md:146`：一个进程只有一个可见流；切换把在途回合从观察者手里摘走，事件无法按 tab 路由。
- `docs/handoff.md:348`：同 workspace 原地 `switch_session` 是**串行复用，不是并发共享**。
- 11-06 / 11-09 的 `scripts/perf/probe-shared.mjs` 实验：A 上 `agent_start` 后切到 B 再切回，切走后 12 个事件里没有 `agent_end`、没有 `message_update`，切回后 `messageUpdatesAfterReturn=0`。**共享单 sidecar 已被否决**，协议改动在 upstream fork，不在 Romp 仓库。
- `docs/handoff.md:354`：`get_session_lease` / `open_session` 返回 success，但只是**串行重挂**，不是并发持有多个会话。

结论：同 workspace 点会话 = 把当前 tab 换成另一个会话，天然没法两个一起跑。

## 3. 判据为什么是 hydrated transcript 历史

| 候选 | 排除原因 |
|---|---|
| `tab.sessionPath` 单独用 | `tab:get-all`（`ipc.rs:122-129`）与 `emit_tab_status`（`lib.rs:123-130`）都不发；`handleSessionInfo`（`use-rpc-events.ts:835-838`）只写 `sessionName`/`sessionId`。renderer 唯一来源是 `openTab` 本地插入（`tabs.ts:622`）。**重启恢复的 tab 为 undefined** —— 用户截图场景。`reconcileTabs`（`tabs.ts:488`）与 `applyTabStatus`（`:798`）虽接收该字段，但 Rust 不发。 |
| 让 Rust 补发 `sessionPath` | `sync_session_binding`（`ipc.rs:1197-1209`）在每次 `get_state` 给**全新 tab** 也写 `sessionFile`（注释："including for a fresh tab's first session"），污染空白判定。 |
| `messageCount > 0` | 新 session 的 `get_state` 返回 `messageCount:1`（本会话 rpc-caps 探针实测）。 |
| `sessionRuntimeStore(tab.id,"session").sessionFile` | 同样被 `get_state` 污染；测试 harness 从未 `registerSessionRuntime`，该调用恒返回 null（`session-runtime-context.tsx:97-99`），断言会绿得毫无意义。 |
| **`useMessagesStore.getState()`** | scoped hook，`getState()` 跟随 `focusedSessionRuntime()`（`:182`），而 `openSessionFromSidebar` 的 tab 正是从 `focusedSessionRuntime()?.tabId ?? tabs.activeTabId` 取的 —— 天然同一个 tab。生产里 `createTabRuntime` 为每个 tab 建 "messages" store、`hydrateTabSession` 往 `sessionRuntimeStore(tabId,"messages")` 写；测试里无 runtime 则回落 default store，现有 `resetAll()`（`:146`）与 `:203` 已用 `useMessagesStore.setState` 灌历史。上游 `TabBar.tsx:478-487` 复合空白谓词亦按 tab 取 scoped store。 |

`liveMessages` 兜住未落盘的在途内容；`tab.sessionPath` 兜住 `openTab` 打开、首次 hydrate 之前的窗口。三者 OR 最坏只多起一个进程，功能仍正确。

## 4. 并行能力已经存在

- `ipc.rs:134-181`：`tab:spawn` 为每个新 tab 起独立 `omp.exe --mode rpc-ui`，`LaunchOptions { resume_session }` 恢复目标会话。
- `ipc.rs:1485` `max_sidecars()`：上限 `ROMP_MAX_SIDECARS`，默认 6；`:1494` `evict_idle_sidecar` 回收 LRU 可释放 tab；`:1475` `is_evictable` 保护 active / streaming / 阻塞 UI / 浏览器页面。满且无 victim 时 `ipc.rs:138-139` 静默返回 `Value::Null`。
- `tabs.ts:565-589`：两种 refusal 已处理 —— `kind-mismatch` 提示并留在原地；`owned`（目标会话已被某 tab 拥有）→ `switchTab(owner)` 或 `sessions.openInNewWindow`，**不会重复开同一会话**。
- `tabs.ts:624` 新 tab 写入 `browsing: true`；`:809` turn 开始时 `applyTabStatus` 清除 `browsing`。
- 本次 `%APPDATA%\dev.romp.desktop\runtime.log` 无任何 eviction 记录，用户现象不是进程回收导致。

## 5. 测试契约（`use-session-switch.test.ts`）

- `:499-513` `seedFocusedTab(blank)`：`blank=false` 时**不伪造 `sessionPath`**，改为给 `useMessagesStore` 灌一条历史消息；主用例断言 `tabs[0].sessionPath === undefined`（真实恢复态）。
- 原 `:508-529`「有历史也原地切」已改写为「2 次 spawn、无 `switchSession`、store 2 个 tab」。
- 新增 `opens a new tab when only unflushed live rows remain`。
- 保留 streaming / blank / un-hydrated / cross-workspace / pending extension UI / queued prompts / unsent draft 全部用例。
- `:1-10`、`:219`、`:339` `switchSessionNow` 的 F-OWN owner-guard 契约，本任务不改。
- `browsing` 是 renderer store 本地标记（`tabs.ts:598`/`:624`），**不进 `tabs.spawn` 载荷**（`tabs.ts:550-556` 只传 `cwd/sessionPath/kind/worktree`）。

## 6. 文档同步位置（8 处）

| 位置 | 现状摘要 |
|---|---|
| `docs/parity.md:45` | 分叉清单：`canBrowseInPlace` 允许任何空闲同 workspace/kind tab 原地切 |
| `docs/parity.md:165` | 新建/切换会话：同 workspace 空闲 tab 原地 `switch_session`，不再新开进程 |
| `docs/architecture.md:14` | 例外清单：`use-session-switch.ts` 常驻 sidecar 原地切换 |
| `docs/architecture.md:146` | 共享 sidecar 否决结论段末：「同 workspace 切会话走 `switch_session` 原地复用」 |
| `docs/handoff.md:23` | 阶段 3 摘要列「原地 `switch_session`」 |
| `docs/handoff.md:71-72` | 关键决策 bullet：同 workspace 空闲 tab 直接 `switch_session` |
| `docs/handoff.md:124` | 分叉清单括号「`use-session-switch.ts`（原地切换）」 |
| `docs/handoff.md:159-161`、`:348` | 决策 8「一个 tab 持有一个 sidecar，同 workspace 原地换线程」；串行复用说明 |

改写时**必须保留** `docs/architecture.md:135-146` 与 `docs/handoff.md:347-360` 的共享 sidecar 否决实验结论；那是不变量，改动的是 UI 默认值。

## 7. 交付面与日志陷阱

- 用户运行 `%LOCALAPPDATA%\Romp` 安装版，`dist` 是 NSIS 打进安装目录的资源；只改源码不重建 + 静默安装，用户看不到变化。
- `ipc.rs` 只 debug 打印 channel 名，`runtime.log` 从不记录命令类型；`openTab` 的 `pendingSessionPath` 在 ready 后会由 `tabs.ts:1072-1073` 合法补发 `switch_session`。**「日志无 switch_session」既不可观测也不成立**。可观测信号是 `tab:spawn` ×2 / `spawning sidecar` ×2 / `omp.exe` ×2。
- `scripts/dev.ps1:38-41` 见到 `romp-desktop` 进程就 throw；`src-tauri/src/lib.rs:866` 的 single-instance 会让带 CDP 参数重启的安装版只聚焦旧窗口后退出。安装前需正常退出，不强杀（`:252` 强杀留 sidecar 孤儿）。
- `docs/handoff.md:124`：上游 renderer 字节级改动仅允许 `main.tsx`、`index.html`、`ChatStream.tsx`、`use-session-switch.ts` 等白名单文件。本任务仍落在 `use-session-switch.ts` 内，不新增分叉文件。
