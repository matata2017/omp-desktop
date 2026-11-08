# Research: cap 与驱逐的源码前提（2026-11-08）

## 1. 源码事实

- `src-tauri/src/ipc.rs:134-140`：`tab:spawn` 在 `state.clients.len() >= max_sidecars()` 且 `evict_idle_sidecar` 失败时返回 `null`。
- `src-tauri/src/ipc.rs:1477-1481`：`max_sidecars()` = `ROMP_MAX_SIDECARS`，**默认 6**。
- `evict_idle_sidecar` 的候选条件：非 active、有 client、**`tab_sessions` 里有绑定 session**、未 streaming、无 pending approval、`browser_pages == 0`。
- 因此：
  - **fresh tab（无 session 文件）不可驱逐**。cap3 + 4 个 fresh tab，第 4 次 `tab:spawn` 会被拒绝（返回 null）。
  - **session-bound tab 可驱逐**。cap3 + 4 个有 session 的 tab，第 4 次 spawn 会驱逐 LRU 空闲 tab 后成功；被驱逐 tab 再次激活需要 resume。

## 2. 样本状态

### cap6（干净，可用）

隔离 `OMP_DATA_DIR` + 3 个**不同**真实 session 副本：4 个 tab 全 ready，`spawns=4 / exits=0 / releases=0`；total private **2455.0 MB**，total RSS 1547.6 MB，4 个 sidecar，单 sidecar private max 520.7 MB。

### cap3（**无效**：respawn 风暴）

同一轮 cap3：`spawns=32 / exits=27 / releases=2 / already_started=false=29`，`runtime.log` 里 `spawning sidecar tab=tab-2` 后 200ms 内跟 `sidecar exited; state cleared tab=tab-2`，持续约 30s；最终 tab-2 仍 `starting`，`stable=true` 只是两次快照恰好一致。**`total_priv=1866.2` 是风暴中的快照，不能作为「cap3 省 666 MB」的结论。**

### 未归因的疑似产品 bug

`already_started=false` 被反复记录 29 次，说明被驱逐 tab 的状态没有进入稳定态（应为 `asleep`），renderer 的 `ensure_tab_started` 继续重试并重启 sidecar。需要单独定位：驱逐后 `pool`/`tab:status` 的状态映射，以及 `ensure_tab_started` 的重试条件。这与 cap 默认值决策强相关。

## 3. 隔离缺口（重要）

- `OMP_DATA_DIR` 只影响 GUI 数据（prefs/layout/runtime.log），**不影响 sessions**；`omp_core::paths::sessions_dir()` 走 `PI_CODING_AGENT_DIR` 或 `~/.omp/agent`。
- 因此探针在真实会话目录创建了 session 文件（本轮清理了 3 个 `probe-*` 与 9 个探针窗口内产生的 ~6.93 MB 副本）；后续实验必须设置 `PI_CODING_AGENT_DIR=<temp>`，并把 3 个不同 session 的副本放进 `<temp>/sessions/--D--rust-omp--/`。
- 选源时排除 `probe-*`，并按每个 JSONL 第 2 行的 `session.id` 断言 3 个 id 互不相同（只按文件名去重不够）。

## 4. 量具修正（本轮踩到的）

- `cdp-eval.mjs` 支持 `@file`：表达式写入 UTF-8（**无 BOM**）文件，绕开 PowerShell 5.1 传参时吞引号/反斜杠（症状：`SyntaxError: Unexpected token ':'`）。
- 顶层 `await` 需要 `replMode: true`；裸 async IIFE 的返回值可能序列化成 `{}`，优先比较副作用（tab 数/状态）而非依赖对象返回值。
- `measure.ps1` 的 `-KeepRunning` 是二次采样前提；默认 `finally` 会 `taskkill /T /F`。
- PowerShell 重定向 `1> file` 是 UTF-16LE；先 `Out-String` 收成字符串再 `ConvertFrom-Json`。

## 5. 隔离会话的尝试失败（2026-11-08）

设置 `PI_CODING_AGENT_DIR=<temp>` 时只建了 `sessions/`，而真实 agent 根还有 `models.db*`、`config.yml`、`agent.db` 等。cap3 与 cap6 都出现 respawn 风暴：`spawns=48 / exits=45 / false_starts=45`，sidecar 在 resume 后约 200 ms 退出。

结论：不是路径根被拒，而是隔离根的模型注册表为空 → `SessionModelRestoreError`（§6）。隔离时必须一并复制 `models.db*`（或 fresh tab 不 resume）；这两个样本作废。

## 6. 手动 sidecar 复现：resume 失败的真实原因

用 `PI_CODING_AGENT_DIR=<temp>` 手动启动 `omp.exe --mode rpc-ui --session <temp copy>`，stderr：

```
SessionModelRestoreError: Could not restore model magpie/raccoon/sn-deepseek-v4-1-flash-1017
```

→ `models.yml:179` 明确定义了 `raccoon/sn-deepseek-v4-1-flash-1017`（带 `-1-`）；失败是隔离根缺 `models.yml`/`models.db*`/`config.yml`，注册表为空。把这几份文件复制进隔离根即可（本轮已按此重跑，见 §8）。风暴放大来自 `ipc.rs:104` 的 `tab:get-all` 会对 active tab 调 `ensure_tab_started`：sidecar 退出后，下一次轮询又把它拉起，cap 低于 tab 数时一次瞬时退出会被放大成几十次重启。

退出日志已接线：`lib.rs` 的 `sidecar exited; state cleared` 现在带 `stderr=`（`SidecarHandle::stderr_tail`），后续不用再手动复现。

## 7. fresh tab 的有效对照（1 轮，clean）

| cap | spawn 序列 | sidecars | total private | total RSS | 结果 |
|---|---|---|---|---|---|
| 3 | 1→2 2→3 3→3 | 3 | 1828.8 MB | 1219.7 MB | 第 4 个 fresh tab 被拒（`tab:spawn` null） |
| 6 | 1→2 2→3 3→4 | 4 | 2327.6 MB | 1455.5 MB | 4/4 ready |

与源码一致：fresh tab 没有 session 绑定，`evict_idle_sidecar` 无法回收，所以 cap < tab 数时第 4 次 spawn 直接被拒。`+499 MB private / +236 MB RSS` 是**被拒的那个 sidecar** 的成本，不是 LRU 回收省下的内存；cap 的内存收益仍要靠 session-bound tab 的成功驱逐来量。

## 8. session-bound cap 矩阵（N=3，clean）

隔离根复制 `models.yml`/`models.db*`/`config.yml`/`agent.db*`；3 个不同真实 session 副本 + 1 fresh active；每 tab 只 `setActive` 一次，固定 12 s settle + 两次 2 s 快照；无 `tabs.list` 轮询。

| cap | sidecars | total private（3 次） | 中位 | all-ready |
|---|---|---|---|---|
| 3 | 3 | 1797.2 / 1806.8 / 1784.8 MB | 1797.2 MB | 否（1 个 tab `starting`） |
| 4 | 4 | 2358.6 / 2347.3 / 2346.1 MB | 2347.3 MB | 是 |
| 6 | 4 | 2352.7 / 2366.0 / 2328.3 MB | 2352.7 MB | 是 |

每次 `clean=true`（`spawns≤6`、`exits≤1`、快照稳定）。结论：cap3 省约 550 MB 但第 4 个 tab 不 ready；4 tab 下 cap4 与 cap6 无内存差异。默认 6 保留，理由是 >4 tab 的余量，而不是 4 tab 的内存收益。
