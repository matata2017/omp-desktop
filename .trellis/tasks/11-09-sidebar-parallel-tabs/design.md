# 设计: 侧边栏会话并行打开

## 1. 边界

只改 renderer 的判定函数、其注释、契约测试与文档。三层保持不变：

1. Rust：`tab:spawn` / `SidecarPool` / `confirmed_switch_path` / `tab_sessions` / LRU 回收 —— 不动。
2. sidecar 协议：一个进程一个活动会话、`switch_session` abort 在途回合 —— 不动，也不尝试共享单进程（11-06 / 11-09 探针已否决）。
3. `requestSessionSwitch` / `SessionSwitchDialog` / `switchSessionNow` —— 不动，⌘P、deep link、对话框维持原语义。

`canBrowseInPlace` 只在 `openSessionFromSidebar` 一处被调用（`use-session-switch.ts:286`），改动面收敛到单文件。

## 2. 判定规则

在 `canBrowseInPlace` 的 cwd 守卫之后增加：

```ts
const messages = useMessagesStore.getState();
if (tab.sessionPath || messages.messages.length > 0 || messages.liveMessages.length > 0) return false;
```

**为什么是 hydrated transcript 历史**（三个被排除的信号都有实证缺陷）：

| 候选信号 | 排除原因 |
|---|---|
| `tab.sessionPath` 单独用 | `tab:get-all`（`ipc.rs:122-129`）与 `emit_tab_status`（`lib.rs:123-130`）都不发该字段；`handleSessionInfo`（`use-rpc-events.ts:835-838`）只写 `sessionName`/`sessionId`。renderer 里唯一来源是 `openTab` 本地插入（`tabs.ts:622`）。**重启恢复的 tab 该字段为 undefined** —— 正是用户截图场景，守卫不触发。`reconcileTabs`（`tabs.ts:488`）与 `applyTabStatus`（`:798`）虽已接收该字段，但 Rust 不发。 |
| 让 Rust 补发 `sessionPath` | `sync_session_binding`（`ipc.rs:1197-1209`）在每次 `get_state` 时给**全新 tab** 也写入 `sessionFile`（注释明说 "including for a fresh tab's first session"），会把空白 tab 一起判成「有内容」，R2 变死代码。 |
| `messageCount > 0` | 新 session 的 `get_state` 返回 `messageCount:1`（本会话 rpc-caps 探针实测）。 |
| `sessionRuntimeStore(tab.id,"session").sessionFile` | 同样被 `get_state` 污染；且本测试 harness 从未注册 runtime，`sessionRuntimeStore` 恒返回 null（`session-runtime-context.tsx:97-99`），断言会绿得毫无意义。 |

`useMessagesStore.getState()` 是 scoped hook，`getState()` 跟随 `focusedSessionRuntime()`（`session-runtime-context.tsx:182`），而 `openSessionFromSidebar` 的 tab 正是从 `focusedSessionRuntime()?.tabId ?? tabs.activeTabId` 取的 —— **天然同一个 tab**。生产里 `createTabRuntime` 为每个 tab 建 "messages" store、`hydrateTabSession` 往里写历史；测试里无 runtime 则回落 default store，现有 `resetAll()`（`:146`）与 `:203` 已用 `useMessagesStore.setState` 灌历史。上游 `TabBar.tsx:478-487` 的复合空白谓词也是按 tab 取 scoped store，语义一致。

`liveMessages` 兜住尚未落盘的在途内容；`tab.sessionPath` 兜住 `openTab` 打开、首次 hydrate 之前的窗口。三者 OR 的最坏结果只是多起一个进程，不会丢会话或 abort 在途回合。

**测试 seed 不伪造 `sessionPath`**：`seedFocusedTab(blank=false)` 改为给 `useMessagesStore` 灌一条历史消息，并断言 `tabs[0].sessionPath === undefined` —— 这才真正复现恢复态。另补一条仅 `liveMessages` 非空的用例。

## 3. 数据流

```
Sidebar.onClick                                  (Sidebar.tsx:588-589)
  → openSessionFromSidebar(session)             (use-session-switch.ts:282)
  → canBrowseInPlace === false                 (history / liveMessages / sessionPath 守卫)
  → tabs.openTab({ cwd, sessionPath, kind, browsing: true })
  → window.omp.tabs.spawn({ cwd, sessionPath, kind, worktree })   (tabs.ts:550-556)
      ├─ 已达上限 6 且无可回收 tab → Value::Null（静默失败，无日志）
      ├─ sessionPath 已被别的 tab 拥有
      │    → { tabId: null, refusal: "owned", ownerTabId }
      │    → tabs.ts:575-579 switchTab(owner)（F-OWN，不重复开同一会话）
      └─ 否则新 TabId + LaunchOptions { resume_session }
           → 独立 omp.exe --mode rpc-ui 进程
  → 两个 tab 各持一个 sidecar，可各自 streaming，互不打断
```

`browsing` 是 renderer store 本地标记（`tabs.ts:598` / `:624`），**不进 `tabs.spawn` 载荷**。

## 4. 兼容性

- **F-OWN 是硬约束**：`switchSessionNow:158-159` 命中 owner → `switchTab`；`openTab` 遇 `refusal:"owned"` 也只 `switchTab`；`ipc.rs:154-157` 直接不 spawn。所以「切到已有 owner tab」不等于「原地 `switch_session`」。
- `openTab` 会设 `pendingSessionPath`，ready 后 `tabs.ts:1072-1073` 在 `currentFile !== sessionPath` 时**合法补发** `switch_session`；`ipc.rs` 只 debug 打印 channel 名，`runtime.log` 从不记录命令类型。故「日志无 `switch_session`」既不可观测也不成立。
- 跨 workspace / kind mismatch / streaming / 草稿 / 审批 / 队列：现有分支本就新开 tab，行为不变。
- 文档里「共享 sidecar 被否决」的实验结论依然成立；改的只是 UI 默认值。

## 5. 权衡

| | 原地切（现状） | 新开 tab（本方案） |
|---|---|---|
| 并行 | 串行；切走会 abort 在途回合 | 两个独立进程可同时 streaming |
| 内存 | 复用，0 新增 | 每会话 +约 500 MB，上限 6 |
| 首次点击 | 无 spawn | spawn + resume（约 0.9–2.5 s） |
| 协议 | 不变 | 不变（不触碰已否决的共享 sidecar） |

用户明确要「两个同时跑」，协议层共享 sidecar 已被探针否决；在 Romp 仓库内多起进程是唯一可行路径。空白 tab 仍复用。未采纳「无条件 openTab」：用户批准的是「空白 tab 仍复用」，且复合判据最坏只多起一个进程。

## 6. 文档同步清单（8 处）

| 位置 | 改法 |
|---|---|
| `docs/parity.md:45` | 分叉清单改为「空白 tab 原地切；有内容 tab 新开独立 sidecar」 |
| `docs/parity.md:165` | 「新建/切换会话」改为「空白 tab 原地切；有内容 tab 新开并行 tab」 |
| `docs/architecture.md:14` | 例外清单改为「空白 tab 原地切换 / 有内容 tab 多开」并说明动机 |
| `docs/architecture.md:146` | 保留共享 sidecar 否决结论，把 UI 默认行为改为多开进程 |
| `docs/handoff.md:23` | 阶段 3 摘要改为「空白 tab 原地切 + 有内容 tab 并行新开」 |
| `docs/handoff.md:71-72` | 关键决策 bullet 改为新默认，注明 11-09 变更与动机 |
| `docs/handoff.md:124` | 分叉清单括号改为「（空白 tab 原地切 / 有内容 tab 新开）」；不新增分叉文件 |
| `docs/handoff.md:159-161`、`:348` | 决策 8 改述；`:348` 保留「串行复用不是并发共享」的协议事实 |

## 7. 风险与回滚

- 6 上限更易触发；`is_evictable` 要求 `has_session`，未绑定会话的空 tab 不可驱逐，满时 `ipc.rs:138-139` 静默返回 `Value::Null`。
- 全新 session 的 transcript 是否含 context 行未单独验证；若含，空白 tab 也会新开进程 —— 功能仍正确，只是省进程的优化失效。
- **交付面**：用户运行 `%LOCALAPPDATA%\Romp` 安装版，`dist` 是打进安装目录的资源。只改源码不重建 NSIS 并静默安装，用户看不到变化。
- 回滚：`git revert` 提交 + 测试 / 文档；协议零改动、无状态迁移；Rust 改动仅限 `tab:spawn` 的 claim 时序，可单独回退。

## 8. 执行期发现：F-OWN 核查与范围扩大（用户批准）

用户日志核查（`runtime.log` 01:43–01:53）显示：`tab-2` / `tab-3` 两次 `tab:spawn` 都带 `session=...01a1b05f...jsonl`，但 `tab-layout.json` 最终是**三个不同** `.jsonl`（tab-2 已迁到 `01a1a5e5`），三个 `omp.exe` 进程并存。**F-OWN 没有失效**：`sync_session_binding`（`ipc.rs:1197-1225`）在 `get_state.sessionFile` 漂移时合法 `release_session(previous)`，旧会话归属被释放后 `tab-3` 才能打开它；这是纠错路径，不是缺陷。日志记的是 `tab:spawn` 请求的 `session_path`（`:162-168`），不是实际绑定。「两个 tab 抢同一会话」列为**待验证假设**，未作为已确认根因。

查出的**可确认缺陷**是 `tab:spawn` 的 check-then-claim TOCTOU：owner 检查在 `:155`、`claim_session` 在 `:187`（旧代码），中间隔着耗时的 `start_tab`，且 claim 结果被 `let _ =` 吞掉。两次并发 spawn 同一会话会各自通过检查、各起一个进程，第二个 claim 静默失败。

修复（`src-tauri/src/ipc.rs`）：在分配 `TabId` 后、`start_tab` 前原子 `claim_session`；失败即返回 `refusal:"owned"`；`start_tab` 失败时回滚 claim 与三个 map。不改归属语义——`release_process`（`pool.rs:181-195`）保留 `session_owners` 是有意的：被回收的 tab 仍注册、重聚焦要 resume 同一 transcript，释放归属反而会让别的 tab 抢走会话。

新增测试 `pool::tests::releasing_a_process_keeps_the_session_binding_for_resume` 钉住该不变量。**待验证假设**：并发两次 spawn 同一会话是否曾真实发生，需带并发的集成测试或复现日志，本任务未做。
