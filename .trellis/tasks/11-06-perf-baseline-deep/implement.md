# 实施计划（收尾状态）

前提：Windows，仓库 `D:\rust-omp`。原始计划见 git 历史中的初版；本文记录实际执行结果与偏差，含 2026-11-08 的一轮 review 修正。

## 0. 验证工作区已有的 watchdog [完成]

- [x] `cargo check -p romp-desktop` 通过（0 errors，仅既存的 unreachable_pattern 警告）。
- [x] 起 debug app，`Stop-Process` 杀 sidecar pid 12464 → `runtime.log` 出现 `sidecar exited; state cleared tab=tab-3 pid=12464`。
- [x] CDP 读 `document.body.innerText`：UI 显示「Sidecar 进程失败 / 重启」与 tab 状态「已退出」。
- [x] `window.omp.app.quit()` 后本仓库 omp 子进程 0 个，无 panic。

原计划的「关 tab 后无二次清理日志」未单独验证，以 quit 后无孤儿覆盖。

## A. 合并测量器 [完成]

- [x] `scripts/perf/measure.ps1` 成为唯一实现：`sidecar ready` 等待、CDP 端口、`-Out` JSON、`-Tabs` 真驱动、退出码 0 / 3（`-File` 下 throw 为 1）、`-KeepRunning`、`-NoDrive`、`-WorkingDirectory` / `-Arguments`（上游 Electron 对照用）、按角色拆进程树。
- [x] `scripts/perf/drive.mjs`：CDP 探测重试、等 `window.omp`、开 tab、读 performance marks、socket 断开重连、显式 `process.exit`（曾因未关闭 socket 挂住测量 3 分钟）。
- [x] 删除 `scripts/measure-perf.ps1`，docs 引用同步改。
- [x] 修 `-Tabs 1` 多起一个 sidecar 的缺陷：drive 先等**应用自己的 active tab** 到 ready，再只激活缺的数量，且不碰 active tab。自然等待用完整 `-ReadyTimeoutSeconds` 预算（不设固定 8 s 短上限，慢机器会漏）。
- [x] 单实例锁按 app identifier，清理按 `romp-desktop.exe` 名字；测量结束 `taskkill /T` 自清理。

验证：release 1/2/4 tab 三次测量，进程树 sidecar 数为 1/2/4，`readyTimedOut=false`，JSON 字段完整。曾有一次坏测量（655 ms / 2 sidecar / 942.3 MB）已排除，不进基线。

## B. 启动分段 [完成]

- [x] `src-tauri/src/lib.rs`：`epoch_ms()` / `boot_mark()`，`process_start` / `window_visible`，`sidecar ready` 补 epoch_ms。
- [x] `src/renderer/main.tsx`：`boot:first-paint`（两次 rAF 后）。
- [x] `src/renderer/components/chat/ChatStream.tsx`：`chat:first-session-rendered`，在 `awaitingTranscript` 为假后触发。
- [x] `measure.ps1` 合并日志与 CDP marks，缺段写 `null`。

验证：代表批次四段 327 / 1861 / 4013 / 4063 ms；干净批次方差 275–694 / 1724–2763 / 3783–5357 / 3838–5420 ms。

## C. 上游对照 [部分完成，如实记录]

- [x] `.research/omp-gui-git` 跑 `bun install --frozen-lockfile`（697 包）+ `bun run build` 成功。
- [x] 同一测量器跑上游，`-NoReady -NoDrive` + 独立 `--user-data-dir`（避开用户已运行的 `C:\Program Files\omp` 单实例锁）。
- [x] 对照表进 `docs/perf.md`：冷启 3218 ms / 1082.9 MB 对 327 ms / 712.0 MB。
- [ ] 未做 `electron-builder --dir` 打包产物，`upstream-build.md` 未单独落盘。上游是未打包的 dev 源码运行，口径差异已写进 `docs/perf.md`，内存数字只作方向参考。

## D. 进程模型实验 [完成，否决]

- [x] `scripts/perf/probe-shared.mjs`：只对 `.perf/probe-sessions/` 里的会话副本发 prompt，sidecar 设独立 `OMP_DATA_DIR`，不碰用户历史。
- [x] 实测：切会话 A → prompt 起流 → 切 B → 切回 A。两次 `switch_session` 都 `success:true / cancelled:false`，切换后 A 的 `message_update` 归零、`agent_end` 不出现，切回也不恢复。
- [x] 结论写 `docs/architecture.md` 决策记录。按停止条件否决，未写共享实现。

原计划的「三组测量」以 release 1/2/4 tab 内存 + 该探针替代，未单独跑 per-tab 双流式。

## E. 流式渲染热点 [完成，按停止条件未改代码]

- [x] `scripts/perf/profile-stream.mjs`：真实 prompt（40 行数字）、CDP Profiler、`agent_end` 停止、metrics 前后差、self-time top 20。
- [x] 实测 TaskDuration 355 ms / ScriptDuration 93 ms / Layout 3 / Recalc 10，self-time 以 idle 为主。40 行回答无渲染热点。
- [x] 报告进 `docs/perf.md`。按停止条件未改 `ChatStream.tsx`（除 B 的 mark）。

## F. UI 审计 [完成，剩余项已记录]

- [x] `scripts/ui/audit.mjs`：locale 扫描（按 `t("key")` 使用频次排序）、CDP 注入 axe-core、`Page.captureScreenshot`。
- [x] `axe-core` 4.14.0 加到 devDependencies（MPL-2.0，dev-only），许可写进 `docs/handoff.md` 许可表；`bun run ui:audit` 脚本。
- [x] 首轮 345 条文案命中，处理 8 个高频键（en/zh/ru 同步），剩 328 条留在 `docs/ui-audit.md`。
- [x] `LangSwitcher` 的 `aria-label` 包含当前语言短名；`ContextStrategyBar` 与 `ContextUsagePopover` 去掉与可见文本不一致的 `aria-label`，交给可见文本命名（WCAG 2.5.3）。
- [x] `ContextUsagePopover.test.tsx` 删掉 2 行钉旧 `aria-label` 字符串的 incidental 断言（其余 textContent / dialog 断言全保留）。曾试过 `aria-hidden` / `display:contents` 变体，axe 按屏幕可见文本判定，无效；也试过恢复 `aria-label` 保测试，但那等于保留真实 a11y 违规，已纠正。
- [x] 最终 release 重生成 `docs/ui-audit.md` + `docs/ui-audit/screen.png`：axe 只剩 `nested-interactive` 3 节点（上游结构，逐条记录理由），`label-content-name-mismatch` 清零。

验证：`bun run ui:audit` 产出报告与截图；`ContextUsagePopover.test.tsx` / `ContextStrategyBar.test.tsx` / locale 定向 24 项通过；全量 `bun run test` 1571 项通过。

## G. 预算门 [完成]

- [x] `scripts/perf/gate.ps1`：`0` 通过 / `1` 超预算 / `2` JSON 缺字段。冷启噪声大，传 `-MaxColdStartMs` 时用绝对阈值、跳过紧相对比较；内存同时用相对 `-MaxRegressionPct` 与绝对 `-MaxRssMB`。
- [x] `docs/perf-baseline.json` 入库便携基线（checkout 后门就能用），`package.json` 的 `perf:gate` 写死 780 MB / 1500 ms。
- [x] 实测当前 release（694 ms / 711.8 MB）exit 0；人为改 900 MB exit 1。

## H. stats 懒启动的连带修正 [完成]

- [x] `ipc.rs` 的 `stats:fetch` 失败改为 **resolve** `{ error, unavailable }`（对齐上游 `main/ipc.ts` 与渲染层 `stats-sync.ts` / `use-stats.ts`），reject 会绕过 90 s 重试预算弹死路错误。
- [x] `crates/omp-stats` 的 `ensure_running` 加 `tokio::sync::Mutex` 串行化，避免懒启动后并发首用双 spawn、互相 `kill_on_drop`。
- [x] `docs/parity.md:190` 与 `docs/architecture.md` 同步「刻意偏离蓝图」的决策与实测。

验证（最终 release，CDP）：干净进程表 0 个 stats 子进程 → `stats.url()` → 1 个；首启 312 ms / 二次 7 ms；杀掉子进程后并发两个 `stats:fetch` 都返回真实数据、无 reject、无 `unavailable`，且只 spawn 1 个子进程。

## 质量门 [全部通过]

- [x] `cargo test --workspace`：158 passed（16 suites，0 failed）。
- [x] `bun run typecheck`：通过。
- [x] `bun run test`：186 files / 1571 passed（0 failed，带完整日志）。一次全量出现 1 条计时敏感偶发失败，JSON reporter 复跑与完整重跑均 1571 全绿，`use-rpc-events.test.tsx` / `TabBar.test.tsx` 连跑 3 次各 79/79 稳定，与本次改动无关。
- [x] 定向测试：`ContextUsagePopover` / `ContextStrategyBar` / locale 共 24 项通过。
- [x] `bunx tauri build --no-bundle` 最终重建，dist 与源码同步；清场后 0 个本仓库 omp 孤儿进程。
- [x] 测量命令重跑：release 1/2/4 tab、懒启动 stats 的 `stats:url` 与并发 `stats:fetch`、预算门两条路径。

## 提交

前三个 work commit（`5c68022` 应用代码、`0f5e41e` 测量器/审计/文档、`a4d1e6a` 任务与 spec）之后是 journal 的 bookkeeping commit（`8269f3c`）。本轮 review 修正作为第 4 个 work commit 追加。Trellis 是 degraded 模式（无 session identity），`task.py finish` 返回 `No current task set`，任务状态与 `/finish-work` 需在 IDE 里收尾；因此 work commit 会落在 bookkeeping 之后，非交错提交。
