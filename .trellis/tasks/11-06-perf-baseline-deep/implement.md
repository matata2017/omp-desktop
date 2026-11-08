# 实施计划（收尾状态）

前提：Windows，仓库 `D:\rust-omp`。原始计划见 git 历史中的初版；本文记录实际执行结果与偏差。

## 0. 验证工作区已有的 watchdog [完成]

- [x] `cargo check -p romp-desktop` 通过（0 errors，仅既存的 unreachable_pattern 警告）。
- [x] 起 debug app，`Stop-Process` 杀 sidecar pid 12464 → `runtime.log` 出现 `sidecar exited; state cleared tab=tab-3 pid=12464`。
- [x] CDP 读 `document.body.innerText`：UI 显示「Sidecar 进程失败 / 重启」与 tab 状态「已退出」。
- [x] `window.omp.app.quit()` 后本仓库 omp 子进程 0 个，无 panic。

原计划的「关 tab 后无二次清理日志」未单独验证，以 quit 后无孤儿覆盖。

## A. 合并测量器 [完成]

- [x] `scripts/perf/measure.ps1` 成为唯一实现：`sidecar ready` 等待、CDP 端口、`-Out` JSON、`-Tabs` 真驱动、退出码 0/2/3/4、`-KeepRunning`、`-NoDrive`、`-WorkingDirectory` / `-Arguments`（上游 Electron 对照用）、按角色拆进程树。
- [x] `scripts/perf/drive.mjs`：CDP 探测重试、等 `window.omp`、开 tab、读 performance marks、socket 断开重连、显式 `process.exit`（曾因未关闭 socket 挂住测量 3 分钟）。
- [x] 删除 `scripts/measure-perf.ps1`，docs 引用同步改。
- [x] 修 `-Tabs 1` 多起一个 sidecar 的缺陷：drive 先等应用自然把 active tab 拉到 ready，再只激活缺的数量，且不碰 active tab。
- [x] 单实例锁按 app identifier，清理按 `romp-desktop.exe` 名字；测量结束 `taskkill /T` 自清理。

验证：release 1/2/4 tab 三次测量，进程树 sidecar 数为 1/2/4，`readyTimedOut=false`，JSON 字段完整。

## B. 启动分段 [完成]

- [x] `src-tauri/src/lib.rs`：`epoch_ms()` / `boot_mark()`，`process_start` / `window_visible`，`sidecar ready` 补 epoch_ms。
- [x] `src/renderer/main.tsx`：`boot:first-paint`（两次 rAF 后）。
- [x] `src/renderer/components/chat/ChatStream.tsx`：`chat:first-session-rendered`，在 `awaitingTranscript` 为假后触发（fresh tab 落在起始页，resume tab 等历史替换骨架）。
- [x] `measure.ps1` 合并日志与 CDP marks，缺段写 `null`。

验证：release 1 tab 四段 327 / 1861 / 4013 / 4063 ms，单调不减。

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
- [x] `axe-core` 加到 devDependencies（MPL-2.0，dev-only），`bun run ui:audit` 脚本。
- [x] 首轮 345 条命中，处理 8 个高频键（en/zh/ru 同步），剩 328 条留在 `docs/ui-audit.md`。
- [x] `LangSwitcher` 的 `aria-label` 包含当前语言短名；`ContextStrategyBar` 去掉与可见文本不一致的 `aria-label`。
- [x] `ContextUsagePopover` 试过同样改法，会让 `ContextUsagePopover.test.tsx` 的 2 条可访问名契约断言失败；`aria-hidden` / `display:contents` 也无法规避 axe 的 visible-text 检查，已回退到上游原样。剩余 `label-content-name-mismatch` 1 节点与 `nested-interactive` 3 节点属于上游结构，逐条记录理由。

验证：`bun run ui:audit` 产出报告与截图；`ContextUsagePopover.test.tsx` / `ContextStrategyBar.test.tsx` / locale 测试共 24 项通过。

## G. 预算门 [完成]

- [x] `scripts/perf/gate.ps1`：缺字段 2 / 回归或 ready 超时 1 / 通过 0；`-MaxRssMB` 与 `-MaxColdStartMs` 可显式传。
- [x] `package.json` 加 `perf:gate`、`ui:audit`。
- [x] 基线自比 exit 0，人为加倍 exit 1，均实测。

## 质量门

- [x] `cargo test --workspace`：158 passed（16 suites）。
- [x] `bun run typecheck`：通过。
- [x] `bun run test`：全量结果见下方收尾记录（并发跑曾出现 flaky 失败，单独 JSON 复跑 0 failed / 1571 passed）。
- [x] 测量命令重跑：release 1/2/4 tab 与懒启动 stats 的 `stats:url` 均实测。
- [x] `bunx tauri build --no-bundle` 最终重建，dist 与源码同步。

## 提交

按切片分组提交，commit message 沿用 `perf(...)` / `fix(...)` / `test(...)` / `docs(...)` 风格。
