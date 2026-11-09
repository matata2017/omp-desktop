# 测量口径审计 — Romp（对象：review worktree `e2a9289`）

范围：`scripts/measure-perf.ps1`、`scripts/measure-click.mjs`、`docs/perf.md`、`src-tauri/src/lib.rs`、
`src-tauri/src/ipc.rs`、`src/bridge/omp-api.ts`、`src/renderer/stores/tabs.ts`。
只读审计，未构建、未启动应用、未修改 worktree。

图例：[F] 源码/工件可核验事实；[H] 假设（需实测才能成立）；[U] 当前无任何测量。

## 1. 八条可核验事实

**[F1] 就绪判据会命中上一次运行残留的日志行。**
`scripts/measure-perf.ps1:81` 计算了 `$startLines`（本次运行前的行数），但全文再无引用；
`:86` 只用 `$tail -match 'sidecar ready'` 在 `runtime.log` 最后 40 行里匹配。同一文件被
`lib.rs:770` 用 `rolling::never` 以追加方式复用（日志实测 111 行、跨 11 次 spawn、11 次 ready），
所以上一轮遗留的 `sidecar ready` 会立即满足条件，采样窗口可能落在进程启动中段——这正是
`docs/perf.md:12` 声称已修掉的「min/max 差 7 倍」现象的真实成因。`[F]`

**[F2] 冷启动只测到「窗口句柄出现」，不是首帧，也不是可用态。**
`measure-perf.ps1:30` 轮询 `MainWindowHandle -ne 0`；`docs/perf.md:22` 据此报 137/141/233/321 ms。
窗口由 `src-tauri/tauri.conf.json:13-22` 的 `app.windows` 静态创建；该配置没有 `create:false` / `visible:false`，也没有首帧后再 `show()` 的初始 gating。`src-tauri/src/lib.rs` 的 `window.show()` 位于 `:661`（托盘动作）、`:786`（第二实例）、`:921`（深链），均是后续唤起，不是初始显示控制。窗口句柄出现因此不覆盖 WebView2 初始化、JS 包加载、React 首帧。`[F]`
`docs/architecture.md:92` 描述的「窗口 `create:false`，pre-paint 后 `show()`」在当前提交未实现。`[F]`

**[F3] 2–4 s 的「剩余时间」被归错了阶段。**
`docs/perf.md:24-26` 把 `romp run() start → sidecar ready` 的 2–4 s 说成「剩余是大头在首个页面渲染」。
但 sidecar 不在 setup 里启动（`lib.rs:948` 只 `register_deferred`），首次 spawn 发生在
`tab:get-all` / `tab:set-active`（`ipc.rs:101-103`、`:216-221`）。
按本机现存日志计算：`run() start → spawning sidecar` = 1.322 / 3.000 / 3.456 / 4.306 s，
`spawn → ready` = 740–935 ms。也就是说 2–4 s 的主体是 **renderer 启动 + 首次 IPC**，
而 `ready → 首条 bubble` 完全没有被这段日志覆盖。`[F]`

**[F4] 唯一存活的原始工件无法复现文档里的数字。**
现存 `%APPDATA%\dev.romp.desktop\runtime.log`：111 行、`spawning sidecar` 11 次、`sidecar ready` 11 次，
但 `stats dashboard ready` 0 次、`host tools registered` 0 次、`released idle sidecar` 0 次，
最后写入 2026-11-04 17:45（本地）。而 `stats` 预启动是 `669d6ed`（2026-11-04 22:05）才加的
（`lib.rs:941`），`docs/perf.md` 的数字出自 `42b7d76`（2026-11-04 23:27）。
结论：`perf.md` 状态①里那 96 MB 的 `omp stats` 子进程，在这份唯一留存的日志里没有任何对应事件；
文档数字的原始采样输出（每轮 min/max、角色明细）没有入库（`git ls-files` 只有脚本与文档）。`[F]`

**[F5] 口径是 WorkingSet，不是 private/commit，且没有 CPU、句柄、线程。**
`measure-perf.ps1:57-75` 只用 `Measure-Object -Property WorkingSet64 -Sum`（`:62-64`），
输出字段却叫 `rss_*`（`:112-114`）。`docs/perf.md:76-78` 自认 WorkingSet 会被 OS trim，
因此 348–973 MB 的区间是同一定义下的正常抖动，不能当作「启动期未完成」的证据。
`private bytes` / `CommitSize` / `PrivateWorkingSet` / CPU 时间 / 句柄数均未采集。`[U]`

**[F6] 进程归属：树遍历合理，但两个计数是全局按名匹配。**
`Get-AppTree`（`:40-55`）从 `romp-desktop` 的 pid 递归 `ParentProcessId`，这部分是对的；
但 `:106-109` 的 `sidecars` / `stats_children` 用 `Get-CimInstance -Filter "Name='omp.exe'"` 全局扫描，
其中 `*rust-omp*stats*` 与真实 argv 不匹配——`crates/omp-stats/src/lib.rs:174-189` 实际启动参数是
`stats --host 127.0.0.1 --port 0 --no-open`，不含 `rust-omp` 路径片段，故 `stats_children` 恒为 0；
`sidecars` 用 `*rpc-ui*` 全局匹配，会把本机其它 `omp --mode rpc-ui`（`dev.ps1` 的
`OMP_SIDECAR=D:\ompgui\resources\omp.exe`）算进来，与 `docs/perf.md:16` 声称的「已避免」相反。
另外树遍历看不到父进程先退出后被系统重挂的 WebView2 helper。`[F]`

**[F7] 脚本没有多 tab 能力；4-tab 那一列是手工产出的，且当时代码与现在不同。**
`-Tabs`（`:4`）只在末尾 `:116` 打印 `tabs_requested`，从不驱动 tab；
`Sample-Rss`（`:57-75`）只对当前 active tab 的树求和。`docs/perf.md:44-55` 的 4-tab 数字因此
来自人工 CDP 点击，不可复现。同时 `ROMP_MAX_SIDECARS` 默认是 **6**（`ipc.rs:1251-1257`），
`docs/perf.md:72` 写的「默认 3」是错的：4 tab 落在回收阈值内，不会触发 `evict_idle_sidecar`，
所以那组数字测的是「无回收时的线性增长」，不能用来验证 LRU 回收效果。`[F]`

**[F8] 桥接层/状态层没有任何渲染侧时间戳，测量只能靠外部 DOM 探针。**
`src/bridge/omp-api.ts:122-168` 的 `subscribe` 只做事件转发；`use-rpc-events.ts:664` 的
`ready` 分支直接触发 `get_state` + hydration，全程无 `performance.mark`。
全仓 `performance.*` 仅出现在 `ThinkingBlock.tsx:110`、`use-throttled-text.ts:8`、`lib/thinking.ts:185,192`
（都是 UI 动效，不是启动指标）。因此「窗口→首帧」「ready→首条 bubble」「switch→首条 bubble」
只有 `scripts/measure-click.mjs` 一个外部探针，而它的 `:67` 判断 `!snapshot.pending` 所依赖的
`pending` 字段在 `:60-66` 的返回对象里根本不存在（恒为 `undefined`），且用「文本包含标题」
判定命中，遇到上一个会话残留的同名 bubble 会提前结束、把延迟量小。`[F]`

## 2. 现有资产能证明 / 不能证明

**可复用（无需改动产品代码）**
- `scripts/measure-perf.ps1` 的进程树求和 + 角色拆分骨架（`:40-55`、`:101`）：是唯一能给出
  「主进程 vs WebView2 vs sidecar」构成的东西，修正 F1/F6 两处后即可继续用。
- `scripts/cdp-eval.mjs` / `cdp-cmd.mjs`：已能稳定驱动 WebView2（CDP 9222），可直接跑
  `Performance.getMetrics`、`PerformanceObserver`、`Runtime.evaluate` 读 `window.omp.*`。
- `runtime.log` 的 UTC 微秒 ISO 时间戳（实测 `2026-11-04T09:40:07.513005Z`）：免费的分段计时器，
  `romp run() start`（`lib.rs:778`）、`spawning sidecar`（`lib.rs:393-430` 区段的 info!）、
  `sidecar ready`（`route_frame` 的 ready 分支）三个锚点可直接算差值。
- `src/renderer/components/layout/InputArea.tsx:1174-1183`：`disabled` 由 `status !== "ready"` 驱动，
  是「可用态」的确定性 DOM 判据（比「窗口出现」强得多）。

**能证明（结论强度）**
- 结构性相对比较：有/无 sidecar、1/4 tab 的增量、主进程与 WebView2 的量级——在 dev 构建内成立。
- sidecar 进程数上限逻辑（cap 6）与回收判据（`ipc.rs:1241-1249` 的 `is_evictable`）的代码事实。
- 冷启动到「窗口出现」这一段的绝对值（137–321 ms，dev 构建）。

**不能证明**
- 发布版任何数字：脚本硬编码 `target\debug\romp-desktop.exe`，且 dev 用 `devUrl=http://localhost:1420`
  （Vite、未压缩、HMR 常驻）。`target/` 目录不存在，`.github/` 不存在，`perf/budgets.json` 不存在
  （`docs/architecture.md:111` 承诺的 CI 门禁未落地）。`Cargo.toml:44-48` 有 release LTO/cgu=1/strip，
  但**没有** `[profile.dev] opt-level=2`，与 `architecture.md:92` 的描述不符。`[F]`
- 首帧/可交互时间（F2、F8）：无标记、无观测。
- 内存的「稳态」：采样 10 × 250 ms = 2.5 s，`ready` 后再等 500 ms 即采；sidecar 在 ready 后仍会
  加载 model catalog / LSP / 索引，`private` 未测，长时漂移 `[U]`。
- 多 tab 隔离与并发：无并发回合上限实现（全仓 grep 无 turn-cap；`stores/queue.ts` 只是渲染层快照），
  所以 architecture 里「默认并发 2、超出排队」在测量上不存在对应指标。`[U]`
- 退出/回收延迟：日志中 `released idle sidecar` 为 0 次，回收路径从未被观测过。`[U]`

## 3. 建议新增的确定性证据（按性价比排序）

1. **分段冷启动（T0–T4，全部可从现有日志+CDP 得到）**
   T0=`run() start`、T1=`MainWindowHandle≠0`、T2=CDP `/json` 出现 `localhost:1420` 目标、
   T3=`spawning sidecar`、T4=`sidecar ready`、T5=DOM 出现首个 transcript 节点 / composer `disabled=false`。
   判据：每段独立报告，禁止合并成「冷启动 = 窗口出现」。
2. **渲染侧锚点（唯一需要产品代码改动的最小项）**
   在 `main.tsx` 顶部与 `use-rpc-events.ts:664` ready 分支各加一次 `performance.mark`，
   通过 CDP `Performance.getMetrics` 读 `FirstContentfulPaint`/`NavigationStart`。不侵入 UI。
3. **内存口径升级**：同时采 `WorkingSet64`、`PrivateWorkingSet`（`Win32_PerfRawData_PerfProc_Process`）、
   `CommitSize`、CPU 时间、句柄数；每状态采样 ≥60 s、≥30 点，报 p50/p95 与斜率，而不是 min/max。
4. **归属修正**：`stats_children` 改为 `Name='omp.exe'` + `CommandLine -match ' stats '`；
   `sidecars` 改为按进程树内 pid 计数，而非全局按名。
5. **就绪判据修正**：只接受 `$startLines` 之后的 `sidecar ready`（或直接看新增的 `tab=tab-1` 行号偏移）。
6. **多 tab 自动化**：用 CDP 循环 `window.omp.tabs.spawn(...)` 造 N tab，再逐步 `tabs.setActive`
   以触发 spawn，记录「N tab 的 sidecar 计数 / 树内存 / 第 N+1 tab 是否触发回收」。
7. **工件入库**：每轮采样输出 JSON（含状态、构建类型、commit、WebView2 版本 `156.0.4314.58`、
   `ROMP_MAX_SIDECARS`、样本数组）落到 `docs/perf/runs/<date>-<sha>-<dev|release>.json`，
   供 `perf/budgets.json` 门禁消费。

## 4. 可证伪指标（每条都能被判失败）

| 指标 | 定义 | 可证伪条件 |
|---|---|---|
| release 冷启到可输入 | `run() start` → composer `disabled=false`，release 构建，n≥5 | 中位数 > 1500 ms 即失败 |
| dev 冷启到可输入 | 同上，dev 构建 | 若 dev 已 < 1500 ms，则「dev 慢 4–10 倍」的假设为假 |
| 每 tab 边际成本 | `tabs.spawn` 后进程树 WorkingSet 增量，N=1→2→3→4，cap=6 | 增量 > 250 MB 或非线性（出现回收）即推翻「每 tab ≈235 MB」 |
| 回收有效性 | cap 设为 2，开 3 tab 后 60 s 内 sidecar 数 | 若仍为 3，`evict_idle_sidecar` 路径未被触发（当前 0 次观测） |
| 空载内存 | 0 tab、无 sidecar、无 stats，release | 若 > 120 MB，则「空闲 ≤120 MB」预算在此实现下不可达 |
| 稳态漂移 | 同一状态 5 min 内 WorkingSet 斜率 | 斜率 > 1 MB/min 即「ready 后即稳态」假设为假 |
| 切换延迟 | 同 workspace 点击 → 首条 bubble，n≥10 | 若 warm 中位数 ≥ cold 中位数，则「原地切换省一次进程启动」无效 |
