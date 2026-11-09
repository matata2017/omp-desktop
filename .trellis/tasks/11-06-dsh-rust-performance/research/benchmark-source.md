# DSH benchmark 原始实现审计：已测场景、口径、断言、混淆因素与 Rust 重做的可复用验证方案

> 证据基线：本地原始仓库 `D:/work/huanghao/dsh-src`，HEAD `687f282e8625c6b8d11ff58211a90524be2e4317`，package version `0.2.1-rc.2`。**不代表官方当前最新版**；本文所有结论只在上述提交上成立。
>
> 方法：只读源码与 Agent Note，未执行任何测试/基准/构建。凡是"推断"一律标 `[INFERENCE]`；凡是源码明确写死的数字，给 `file:line`。

---

## 1. 一页结论

1. **门禁 benchmark 共 10 个文件（7 个 Host `*.bench.ts` + 3 个 Client `*.bench.client.ts`）+ 1 个内存压力 runner + 3 个手工 perf 清单。** 其中 `agent-continuation/synthetic-history.bench.ts` 不是计时用例，而是**合成 fixture 的语义自检**（§4.12）。它们按"被测用户路径"组织，而不是按包组织（`benchmarks/AGENTS.md:5`）。
2. **口径统一且被写死**：构建产物（`lib/` + `benchmarks/.dsh-build/`）→ 纯 Node 子进程（无 TS loader）→ 私有 `mkdtemp` 根 → 每端点 3–5 个独立样本 → 取 **median**（少数端点取 **min** 或 **best-of-N**）→ 预算 = 参考期望 × **2**（CI 时间尺度）× **1.25**（方差余量）→ `recordTimings` / `recordPeakMemory` 写 `.dsh-report/report.html`（`benchmarks/support/calibration.ts:4,6,13-15`、`benchmarks/AGENTS.md:7,10,11,12,13`）。
3. **benchmark 故意不断言业务语义**：它只要求"被测调用跑到指定端点"，语义由功能测试/快照语料承担（`benchmarks/AGENTS.md:15`、`2026-09-04-session-open-performance-gate.md:38`）。但每个 case 都带**结构性计数断言**（items 数、wireViews 数、tool call 数、messages 数、可见 tail 数、`pageErrors==[]`），这些才是"跑歪了立刻红"的护栏。
4. **可复用的确定性机制已经很完整**：5 个合成 Session 生成器（含常数量化的分位锚）、`llm-replay`（`ReplayEntry`/`overrideFile`/`childFiles`/`paceMs`/`assertConsumed`）、`launchWebScaffold`（真 shipped composition + 私有世界 + 回环 HTTP + 真 `seedSession`）。这些与语言无关，**Rust 实现可以直接吃同一批字节**。
5. **对"Rust 重做是否更快"最关键的事实**：现有端点覆盖的是 **Host 侧计算与 I/O**（open/read/restore/projection/list/search/fork/resume/terminal ingest/user-text/reconnect/fold），而**浏览器渲染、React fold、输入流**这些端点不会因为 Host 换语言而变快（除了少传字节、少一次往返）。当前 benchmark 也**没有**测量：进程启动到 ready、常驻 RSS 门禁、多会话并发、崩溃恢复。这几项才是 Rust 的潜在收益区，也是必须新增端点的地方。
6. **判定"更快但不损失正确性"必须双门**：① 跨实现 golden digest 等价（同一 fixture 字节 → 同一派生结果）；② 同机、同端点、同统计量（median）的 A/B，且**禁止**把 CI 预算的 2× 机器尺度带入 A/B 比值。缺任一门，"更快"不可采信。

---

## 2. 资产地图

| 资产 | 位置 | 性质 |
|---|---|---|
| 门禁编排 | `vitest.bench.config.ts:12-28` | `fileParallelism:false` + `maxWorkers:1` + `globalSetup` 清空并渲染报告 |
| 门禁用例清单 | `vitest.bench.config.ts:19-22` | 10 个文件：7 个 `*.bench.ts` + 3 个 `*.bench.client.ts` |
| 手工 perf 编排 | `vitest.web.perf.config.ts:7-20` | 继承 `vitest.web.config.ts`，额外 `--expose-gc`，只 include 两个 `.perf` 文件 |
| worker 构建 | `benchmarks/tsdown.config.ts:1-76` | 每目录一个 `outDir`，`neverBundle: /^@deepseek-ai\//`，host/client 两套 tsconfig |
| 子进程 launcher | `benchmarks/support/built-worker.ts:30-101` | 纯 Node 子进程 + 预算/输出校验 |
| 预算换算 | `benchmarks/support/calibration.ts:1-15` | `CI_TIME_SCALE=2`、`PERFORMANCE_BUDGET_HEADROOM=1.25` |
| 缩放报告 | `benchmarks/support/scaling-report.ts:1-294` | `recordTimings`/`recordPeakMemory`/HTML 缩放模型 |
| 机器档案 | `benchmarks/support/machine-profiles.ts:30-58` | PassMark 单线程 + 4KiB QD1 延迟估计 |
| 内存压力 | `scripts/run-memory-pressure-bench.ts:1-147` | cgroup v2 + swapless，4/8 GiB |
| CI job | `.github/workflows/ci.yml:206-262`、`scripts/run-ci-bench.ts:11-26`、`scripts/run-gates.ts:309-310`、`package.json:94` | `node 24 / benchmarks` 独占 runner |
| npm 入口 | `package.json:24,65-67,81-82` | `build:bench`、`test:bench`、`test:bench:memory`、`test:web:perf` |
| 治理规则 | `benchmarks/AGENTS.md:5-18` | 全部方法学约束的权威来源 |
| 回放适配层 | `packages/test-support/llm-replay/src/index.ts`（1209 行） | 无 Key 的确定性模型回放 |
| Web 世界 | `apps/web/tests/scaffold.ts:476-...`（1796 行） | 真组合 Host + 回环端口 + 私有根 |
| 手工诊断 | `apps/web/tests/complex-history.perf.ts:1-1454`、`packages/client/ui-conversation/tests/history-transport.perf.client.ts:1-617`、`packages/session/session-persistence-jsonl/tests/catalog-migration.perf.ts` | **无时间断言**，只报数字 |

---

## 3. 运行口径（harness 的真实语义）

### 3.1 编排层

- `fileParallelism: false`、`maxWorkers: 1`，注释写明"a measurement never shares the CPU with another benchmark"（`vitest.bench.config.ts:9-10,23-24`）。
- `globalSetup` 指向 `benchmarks/support/scaling-report.ts`：`setup()` 删掉 `.dsh-report/`（`:106-108`），`teardown()` 读 `results.jsonl` 渲染自包含 HTML（`:111-126`）；即使 case 失败也会渲染（同段落注释 `:5-7`）。
- `testTimeout: 600_000`、`hookTimeout: 120_000`、`disableConsoleIntercept: true`（`vitest.bench.config.ts:25-27`）。
- `execArgv` 只加 `--no-webstorage`（`vitest.shared.ts:9`），避免进程级 Web Storage 遮蔽 jsdom。

### 3.2 构建与加载边界（这是"数字可信"的前提）

- 计时 CPU 工作**必须**跑在 `benchmarks/.dsh-build/` 的编译产物里，用纯 Node，**不带 TS loader**，且运行时包 import 必须解析到 `lib/`（`benchmarks/AGENTS.md:7`；Note 里给出了拒绝理由：source loader 会改变模块解析与启动行为，`2026-09-04-session-open-performance-gate.md:109`）。
- `runBuiltBenchmarkWorker` 强制实现该约束：worker 必须是 `.js/.cjs`（`built-worker.ts:33-35`）、启动前 `delete NODE_OPTIONS` 与 `TSX_TSCONFIG_PATH`（`:36-38`）、超时 `SIGKILL`（`:58-61`）、只取 stdout 最后一行以 `{` 开头的 JSON（`:70-74`）、返回 `exitCode/signal/timedOut/stderr`（`:75`）。
- worker 内部再自证一次：`assertBuiltBenchmarkRuntime` 要求 `import.meta.url` 落在 `/benchmarks/.dsh-build/` 且包入口匹配 `/lib/**/*.js`（`built-worker.ts:88-101`；调用样例 `session-open.worker.ts:293-295`）。
- 允许的例外被明确限定：**当被测用户路径没有任何公开 Node export 时**，可以把"私有集成适配器"编进 worker，但包 import 必须保持 external（`benchmarks/AGENTS.md:16`）。实例：`benchmarks/terminal-io/session-adapter.ts:1-2`（只 re-export 一行）、`conversation-fold.worker.client.ts:11-30`（13 个 client 侧 fold 模块）、`session-open.worker.ts:31-33`（history controller + model-selection projection）、`active-stream-reconnect/reconnect.worker.client.ts:6-7`。

### 3.3 采样与统计

| 维度 | 事实 | 证据 |
|---|---|---|
| 进程隔离 | 每个样本一个全新子进程；私有 `mkdtemp` 根，成功或失败都清理 | `benchmarks/AGENTS.md:10` |
| 样本数 | session-open 5；session-corpus list/fork 3、search 1、longest fork 1；conversation-fold fold 3（best-of）、preparing 3；terminal-io 5；user-text 5（每样本多次求均值后取最小）；reconnect 3；long-session-browser 3（每次全新 browser + scaffold） | `session-open.bench.ts:36`、`session-corpus.bench.ts:33`、`conversation-fold.bench.client.ts:23,51`、`terminal-io.bench.ts:10`、`user-text.bench.client.ts:13`、`active-stream-reconnect/reconnect.bench.client.ts:12`、`long-session-browser/long-session.bench.ts:14` |
| 统计量 | 默认 **median**；fold 用 **best-of**（`bestOf` 取最小，`conversation-fold.worker.client.ts:168-179`）；user-text 用"每样本均值的最小值"（`user-text.bench.client.ts:112-117`） | 同左 |
| 聚合规则 | 分页端点取"每样本最慢页"的中位数（`long-session.bench.ts:446` 与 `2026-09-06-frontend-performance-budgets.md:25`） | 同左 |
| 内存 | 端点前双 GC → 采样；端点后再双 GC（结果对象保持可达）→ `heapUsed`/`external`/`arrayBuffers`/`rss`/`maxRSS` | `session-open.worker.ts:110-129,182-202` |

### 3.4 预算代数（必须原样复刻，否则不可比）

```
ciTimeBudget(expectedMs) = ceil(expectedMs × 2 × 1.25)          // calibration.ts:4,6,13-15
```

- 时间尺度 `2` 是"x64 CI runner / arm64 参考机"的**实测**比值（`calibration.ts:3-4`；证据 `2026-09-04-session-open-performance-gate.md:40`：CPU-heavy 墙钟为参考机 1.95–2.06×，两次 CI median 差异 ≤5.2%）。
- **例外端点直接钉死托管 CI 实测值**，不再乘 2：current-gen `open` 50 ms→63（`session-open.bench.ts:62-64`）、first-open agent-resume 562（`:70-71`）、request-history 297（`agent-continuation.bench.ts:28-29`）、浏览器 open/page/trajectory 900/700/520→1125/875/650（`long-session.bench.ts:17-22`）。
- **内存与无量纲比值只乘 1.25，不乘时间尺度**（`benchmarks/AGENTS.md:12`）。实例：agent retained heap 26.1→33 MB（`session-open.bench.ts:74-77`）、reconnect retained 24→30 MiB（`reconnect.bench.client.ts:11,58`）、fold delta-scaling 2.5→3.125（`conversation-fold.bench.client.ts:48-49`）、projection-list retained 240→300 MB（`projection-list.bench.ts:19-20,71`）。
- **环境变量不得覆盖预算**（`benchmarks/AGENTS.md:13`）。
- 每个文件还有总时限：session-corpus 300 s 且最后一个 case 断言它（`session-corpus.bench.ts:28-31,330-334`）。

### 3.5 报告与缩放（读数字时必须知道它在做什么）

- 时间端点两种形态：`{ms, cpuMs}`（进程 CPU 时间）或 `{ms, ioShare}`（**声明的**存储等待占比）（`scaling-report.ts:51-58`）。
- 缩放公式（`scaling-report.ts:9-14`，实现 `:214-222`）：
  ```
  ms × ((1 − io) × cpuScore(measured)/cpuScore(target) × coreFactor + io × ioLatency(target)/ioLatency(measured))
  io   = cpuMs 可用时 max(0, 1 − cpuMs/ms)，否则用声明值
  busy = cpuMs 可用时 max(1, cpuMs/ms)，否则 1
  coreFactor = max(1, busy/cores(target)) / max(1, busy/cores(measured))
  ```
- 机器档案的 `cpuScore` 是 PassMark 单线程（2026-10-07 取自 cpubenchmark.net），`ioLatencyUs` 是"典型 4 KiB QD1 同步延迟"的**估计**，且明确写着跨 ISA、突发 vCPU、混合核都"coarse"（`machine-profiles.ts:4-13`）。
- 报告页自己声明：内存**不建模**、峰值内存**没有**在 OS 限制下跑过（`scaling-report.ts:16-17,190`）。
- `recordTimings` 在断言**之前**调用（`benchmarks/AGENTS.md:11`），所以失败 run 也有数据。

---

## 4. 已测场景逐项

### 4.1 `session-open`（Host：打开一个大 Session）

工作负载：200 turns × (500 text + 125 reasoning) deltas = **127,400 事件**（`session-open.bench.ts:33-34`）；输入是**直接构造的 released-v0 物理行**，1 row/frame，Zstd 帧由生产 `compressZstdFrame` 生成（`synthetic-released-v0-session.ts:37,166-192`）。

两个生命周期 × 四个场景（`session-open.bench.ts:290-305,401-495`）：

| 端点 | 计时区间 | 参考期望 → CI 预算 | 证据 |
|---|---|---|---|
| `phases.openMs` | `persistence.open()`（first-open 含 V0→当前代迁移） | 220→550；reopen 50→63 | `:51-64` |
| `phases.readMs` | `handle.read()` + `close()` | 8→20 | `:52,65` |
| `phases.sessionRestoreMs` | `SessionPreparation.create` + `interruptedTurnClosers` | 24→60 | `:53,66` |
| `phases.projectionMs` | `sessionProjections.hydrate()` | 14→35 | `:54,67` |
| `first-history.totalMs` | `SessionHistoryController.follow()` 到第一个 `snapshot` | 220→550 / 48→120 | `:55-57,68-69` |
| `agent-resume.totalMs` | `ctx.agents.resume()` 到 handle 返回 | 562（钉死）/ 40→100 | `:58,70-72` |
| `agent-resume.retainedHeapMb` | 端点后双 GC 的 `heapUsed` 差 | 26.1→33 MB | `:74-77` |

额外两个正交检查：
- **128 MB old-space 下必须跑完**（不记时，只验证不 OOM）（`:40,434-442`）——理由：避免极限 GC 混入正常基线，同时暴露"单样本 heapUsed 看不见的瞬时分配尖峰"（Note `:23`）。
- **未计时的语义前置**：迁移后 1 条 `system/message`、message 派生内容逐条相等、V0 字节不变、successor 可重开（`session-open.bench.ts:352-398`）。

`ioShare` 是**声明值**：`{file:0.1, memory:0}`（`:85`），因为该 worker 只报整段 CPU 时间。

### 4.2 `session-corpus`（Host：多长会话的 list / search / fork）

- 语料规模：list 3,000；search 1,000（独立根）；fork 1,000（独立根，避免 fork 子进入 search 索引）（`session-corpus.bench.ts:23-27`）。
- 端点：`bootMs` / `firstMs` / `repeatMs`（list、search 同构，`session-corpus.worker.ts:275-337`）；fork 逐 rank 的 `forkMs`，覆盖 10 个长度分层中点 + p99 的顶层 rank + 最长（`session-corpus.bench.ts:36-41,184-190`）。
- 断言：所有返回行**必须带 cached projections**（`itemsWithProjections === CORPUS.list`，`:261`）；search 两次不同词且都非空（`session-corpus.worker.ts:321-323`）；fork 覆盖的每个 rank 必须出现在报告里（`:299-303`）；峰值 RSS 取样本最大（`:260,323`）。
- 预算（托管实测钉死 + 1.25）：`listBoot 2400→3000`、`listFirst 3300→4125`、`listRepeat 2700→3375`、`searchFirst 145000→181250`、`searchRepeat 3500→4375`、`forkStratumMedian 100→125`、`forkP99 1100→1375`、`forkLongest 20400→25500`、`peakRss 750/1200/750→938/1500/938`（`:45-72`）。
- 关键负事实（直接可用于"值不值得重写"）：content search 首次构建索引在托管 CI 上 **144 s**（`:49`），fork 最长会话 **~20 s**（`:53`），list 3,000 首次 **3.2 s**（`:47`）。这些是 Host 侧单线程 JS 的实测成本。

### 4.3 `projection-list`（Host 队列响应性）

- 三个负载：`modest{50,100}`、`tail{300,1000}`、`cheap{3000,0}`（`projection-list.bench.ts:24`；worker `:29-33`）。
- 端点：`listAndJsonMs`（list + `JSON.stringify`）/ `listMs` / `worstCallbackDelayMs`（Host 线程上其他回调被 list 挡住的延迟）/ retained heap（`projection-list.worker.ts:222-289`）。
- 观测手法很特别且可复用：patch `scheduler.yield` 计数（`:226-232`）+ 在每次 `wire` 视图调用后 `setImmediate` 测排队延迟（`:237-252`）+ `monitorEventLoopDelay({resolution:1})` 取 `histogram.max`（`:223,273`）。
- 预算：`ciTimeBudget(50/500/150)` = 125/1250/375 ms；callback delay `ciTimeBudget(30)` = 75 ms；retained heap 300 MB（`:68-72`）。
- 语义护栏：`items === sessions`、`wireViews === sessions`、每个 item 都有该 projection key（`projection-list.worker.ts:267-269`；bench `:85-94`）。

### 4.4 `agent-continuation`（Host：长历史请求构建 / 冷续跑 / 子目录发现 / SDK profile）

- 负载常量集中在 `workload.ts:10-20`：800 历史 turns、每 turn 4 tools、tool result 2048 chars、40 请求 turns、20 续跑 turns、100 profile turns、每 live turn 8 tools、16 个 child × 80 turns。
- 端点与预算：`request-history` 297 ms（钉死，来自 238×1.25，`:28-29`）；`tool-continuation` 900→1125；`catalog` 900→1125；`profile-continuation` 1700→4250；tool-continuation retained heap 23→~28.75 MB（`:22-30`）。
- 语义护栏（很有价值）：模型适配器**统计请求数**、工具**统计调用数**，必须精确等于预期，否则抛错（`agent-continuation.worker.ts:113-116`）。这是"跑短了会红"的硬约束。
- `request-history` 是 in-process 构建 40 个 model request，不做 provider 序列化、不发网络（`agent-continuation.worker.ts:40-45`，CASES 文案 `agent-continuation.bench.ts:36-39`）。

### 4.5 `conversation-fold`（Client：历史折叠 + 工具参数渐进发布）

- 配对的合成窗口：200 replies × 2,000 deltas（大）/ 200 × 100（小），两者**事件数与 compact record 数完全相同**，只有 delta 数差 20×（`conversation-fold.worker.client.ts:93-157,284-286`）。
- 判定双指标：`largeFoldMs ≤ 40 ms`（16→40）与 `scaling ≤ 3.125`（2.5×1.25）（`conversation-fold.bench.client.ts:40-49,89-92`）。理由写在 `:36-38`：记录数正比的实现约 2.5×，逐 delta 回放会到 ~11×。
- 另两个 case 测"流式工具参数发布"：write 512 KiB（300→750 ms、30 MB→37.5 MB）、bash 128 KiB（150→375 ms、8 MB→10 MB），并断言 `progressKb`/`filePath`/`detail` 摘要文本（`:53-55,108-114`）。

### 4.6 `terminal-io`（终端环形缓冲不随容量和分块退化）

- 端点：1 MiB 输出注入到 128 KiB 与 4 MiB 保留窗口的 `ingestMs` / `completeMs`（20→50、50→125），**容量比 ≤4×**（`:43-65`，`MAX_CAPACITY_RATIO` `:14`）；5 MiB 完整发送 120→300 ms（`:79-87`）；16 B 小块与 OSC 过滤两种极端只查 retained heap ≤16 MiB（`:67-77`）。
- 语义护栏：必须 `waitReason==='inferred_idle'`、`truncated`、viewport/read 字节数精确等于 `min(256KiB, capacity)`（`terminal-io.worker.ts:79-85`）。

### 4.7 `user-text`（安全脱敏扫描的时间/分配预算）

- 8 个无凭据文档 + 7 个含凭据文档 + 10 个对抗输入 × 2 尺寸（`user-text.bench.client.ts:34-78`）。
- 口径特殊：**每样本对同一 workload 多次求均值，取 5 个样本均值的最小值**（`:112-117`）；`UNMATCHED_RATIO_HEADROOM=1.75` 覆盖 V8 tiering 的观测波动（`:15-20`）；对抗输入要求"加倍输入 ≤2.5× 时间"（`:59-61`）。
- 分配口径：固定 256 MB semi-space，使分配区间内不发生 scavenge（`:23-24`；launcher 参数 `built-worker.ts:44-47`），再按"凭据节点 / 换行 / 掩码字符"三项线性预算断言（`:63-73,154-162`）。

### 4.8 `active-stream-reconnect`（Client：重连时重建未完成回复）

- 100,000 delta reasoning 前缀 → 2 条 compact record；只计 `ClientAssistantStream.replace()`（`reconnect.worker.client.ts:23-46`）。
- 预算：50→125 ms；retained 24→30 MiB（`reconnect.bench.client.ts:9-12`）。
- 语义护栏：`nextFrame === 'transient'`（下一帧仍被接受）、`entries > 0`（`:51-52`）。

### 4.9 `long-session-browser`（浏览器：长会话打开 / 翻页 / Trajectory / 流式输入）

- 单一串行工作流：打开 240-turn 会话 → 逐页点 "Load earlier" 直到全部 → 首次 Trajectory → 回 Chat → 提交后边流式边打字（`long-session.bench.ts:344-453`）。
- 端点预算：`open` 900→1125、`page`（最慢页的中位数）700→875、`trajectory` 520→650、其余 `REFERENCE` 走 `ciTimeBudget`，其中 `streamWall` 额外加 `(DELTAS+4)×PACE_MS` 的脚本化节奏（`:16-27,446-450`）。
- 计时细节（直接决定数字含义）：
  - `open` 从点击会话项开始，到出现 tail 与 composer 为止（`:370-374`）；**启动、播种、侧栏展开都排除**。
  - `painted()` = 两个 `requestAnimationFrame` 回调，注释明确"a rendering opportunity, not a GPU presentation timestamp"（`:34-37`）。
  - 翻页先等按钮 attached、断言**恰好一个** handle，再开始计时，避免把 selector 重试退避算进去（`:47-60`；Note `2026-09-06-frontend-performance-budgets.md:17`）。
  - 流式主线程成本来自 CDP `Performance.getMetrics` 的 `TaskDuration` 差值（`:62-67`），堆来自 `HeapProfiler.collectGarbage` 后的 `JSHeapUsedSize`（`:421-425`）。
  - 输入重叠用 `data-benchmark-input-overlap` 面板见证真实 `isTrusted` 且"已见 FIRST 未见 DONE"（`:108-120,405-413`）。

### 4.10 手工 diagnostics（非门禁，但方法论参考）

- `apps/web/tests/complex-history.perf.ts:1-4` 开头就写"reports measurements without timing assertions because host speed is not a correctness contract"，只保留**结构性断言**（1,000 侧栏、500 turns、2,100 trajectory rows 等 `:39-59,1233-1294`）。它用 CDP 同时报 `TaskDuration / ScriptDuration / LayoutDuration / RecalcStyleDuration / Nodes / JSEventListeners / JSHeapUsedSize`（`:557-583`）。
- `history-transport.perf.client.ts` 对 416,756 逻辑项比较"原始事件 vs 紧凑记录"的 wire 字节、gzip/brotli、解析、校验、fold 时间与峰值堆，并做**等价性 digest 断言**：`digest(compactFold) === digest(rawFold)`（`:31-35,458-470`）。这是"两种表示必须折出相同快照"的原型。
- `packages/session/session-persistence-jsonl/tests/catalog-migration.perf.ts` 同类，包内、非门禁（`benchmarks/AGENTS.md:3`）。

### 4.11 内存压力（`test:bench:memory 4g|8g`）

- 机制：先在无限制环境构建，再用 `systemd-run --user --scope -p MemoryMax=<N>G -p MemorySwapMax=0` 重跑固定 4 个文件（`run-memory-pressure-bench.ts:11,20-25,46-53`）。
- 运行前强校验 `memory.max`、`memory.swap.max`、`process.constrainedMemory()` 三者都等于建模值（`:85-90,101-107`），否则直接拒绝——**防止"以为限了其实没限"**。
- 运行后报 `cgroupPeakMb`、`oomKills`、Node 从 cgroup 推导的默认 heap 上限（`:111-121`）。
- 记录：无限 / 8 GiB / 4 GiB 三次，38 个 case 全过、无 OOM；4 GiB 下 list 首次从 1,235 ms 涨到 2,613 ms（`2026-10-07-memory-limited-benchmarks.md:25-32`）。
- 平台限制：需要 Linux cgroup v2 + systemd user manager（`:132-134`），macOS 只能进容器（Note `:42`）。

### 4.12 `synthetic-history.bench.ts`（fixture 自检，不计时）

它不是性能用例，而是**合成 fixture 的语义回归**：把 `agent-continuation/workload.ts` 与 `long-session-browser/synthetic-history.ts` 两族生成器的输出，经 `parseSessionLog`（`llm-replay/src/index.ts:200-209`）解回当前格式，然后断言

1. 前四类事件必须是 `turn/start, step/start, system/message, user/message`（`synthetic-history.bench.ts:17`）；
2. 第一个 surface node 指向 system head（`:19-20`）；
3. 新一轮 prompt 通过 `surfaceOp: {op:'replace', startSeq, endSeq}` **替换** head 后，`restored.surface.nodes[0] === replacement.seq` 且 `deriveMessages()` 只剩新 system 文本 + 原历史（`:25-33`）；
4. `session/title.messageSeqs[0]` 必须指向 `user/message`，`tool/result.sourceEventSeqs[0]` 必须指向 `tool/call`（`:34-37`）。

对 Rust 重做的意义：在 benchmark 树里，这是**唯一**把"合成输入 → 解析 → 结构语义"串起来的用例（其余 bench 只查端点与计数），也是跨实现等价性最容易照抄的模板（§8.3）。

---

## 5. 可复用的确定性输入与回放机制

这一节是"Rust 侧能直接吃"的清单。共同特征：**输入由审阅过的常数合成，绝不使用录制的 Session / 用户材料 / 环境仓库 / 网络**（`benchmarks/AGENTS.md:9`）。

### 5.1 五个合成 Session 生成器

| 生成器 | 产物 | 关键常量 | 证据 |
|---|---|---|---|
| `writeSyntheticReleasedV0Session(root, shape)` | released-v0 物理行（1 row/frame，Zstd），返回 `{path, compressedBytes, logicalBytes, events, rows, frames}` | 200 turns × 500 deltas；`ROWS_PER_FRAME=1` | `session-open/synthetic-released-v0-session.ts:37,166-192` |
| `SyntheticCorpusWriter.author / writeCorpus` | 22 个 anchor body + N 个 Session（每 Session = 1 header frame + 该 anchor 的 body frames） | 见 5.2 | `session-corpus/synthetic-corpus.ts:386-423,455-483` |
| `syntheticHistory()`（浏览器） | 240 闭合 turns 的当前代 JSONL，含 `{{sessionId}}`/`{{cwd}}` 占位符 | 12 字符 delta、每 6 turn 一个 tool、每 12 turn 一个 60 行代码块 | `long-session-browser/synthetic-history.ts:9-20,25-87` |
| `syntheticHistory(turns)`（Host） | 800 turns 事件数组，走**生产** `Session.append` + `AssistantStreamAccumulator` 得到真实 compact rows | `WORKLOAD` 全表 | `agent-continuation/workload.ts:10-20,66-105` |
| `synthesizeWindow(turns, deltas)`（Client fold） | `SessionEventLikeEntry[]`，成对大小窗口 | 事件/记录数必须相等 | `conversation-fold.worker.client.ts:93-157,284-286` |

复用的三个"确定性"手段：
1. 固定时钟 `TIME_ZERO = 1_700_000_000_000`（每个生成器都有），事件时间 = `TIME_ZERO + seq*1000`（`synthetic-corpus.ts:19,192`；`workload.ts:23`）。
2. 固定 id 规则：`bench-00001`、`prompt-N`、`reply-N-M`、`call-N-M`（`synthetic-corpus.ts:70-72,217-263`）。
3. 固定布局：目录 = `--bench--/bench-session`（`session-open.constants.ts:5-12`），文件名 = `session.jsonl.zstd`。

### 5.2 分布锚定（把"真实的尾巴"搬进合成语料）

`corpus-shape.ts:7,14-25`：22 个上分位锚（10%…100%），三维度（逻辑事件 / 解压字节 / turns），来自 2026-09-28 单机 1,650 个本地 Session 的**聚合**统计（明确只取计数与字节总数，无内容/身份/路径）。

采样规则 `sessionShape(rank,count)`：`quantile = 100*(rank+1)/count`，取第一个 `>= quantile` 的锚（`:69-76`）——保证"合成语料的每个分位都不低于实测分位"，且三维共享同一 rank（"长会话在每个维度都长"）。代价已知：1,000 个 Session 时比实测分位函数多 16% 事件 / 14% 字节（Note `2026-09-28-session-corpus-performance.md:26`）。

其他被锚定的实测比例：19 Sessions/项目目录、每 6 个一个 subagent child、system prompt 20,000 字符、delta 平均 3.9 字符、每 Zstd 帧 1.6 事件（`synthetic-corpus.ts:24-31`）。

### 5.3 确定性伪随机与文本池

- `mulberry32`（`synthetic-corpus.ts:85-95`）——仅用于构造 fixture，不是生产算法。
- Zipf 词表：8,192 词 × 2,000,000 词的共享池，二分查累积分布（`:98-131`）。压缩比 2.8 < 实测 3.4，即"故意存得比真实更占空间"（Note `:28`）。
- 载荷自动校准：对 payload 字符数做 secant 迭代，直到字节数达标（最多 8 次），把每次尝试的 `bytesPerChar` / `bytesPerEvent` 作为下一次的模型（`synthetic-corpus.ts:386-423`）。这是"给定分位目标自动反推工作量"的通用技巧，Rust 侧可直接复用同一批已生成字节，无需重写。

### 5.4 `llm-replay`（无 Key 的确定性模型边界）

- 条目类型：`{kind:'chunks'|'throw'|'hang'}`（`llm-replay/src/index.ts:64-71`）。
- 配置面（`:125-163`）：
  - `file`：主 fixture（`session.jsonl` / `session.vN.jsonl`）；
  - `overrideFile:140`：**裸 `ReplayEntry[]` 直接替换脚本**，或 `{patches}` 只替换指定 call index；
  - `childFiles:147`：嵌套 agent 的子会话日志，按 `createdAt` 排序绑定；
  - `providers:153`：回放专用 provider 目录（决定 `listProviders/listModels/resolveModel` 的可见结果）；
  - `paceMs:154-162`：**每 chunk 的延迟**，注释写死"A realism knob only — correctness must never depend on it"。
- `ReplayHandle.assertConsumed()`（`:171-180`）：teardown 时若"记录的调用没被全部绑定/消费"就报错——**这是防止行为悄悄变少的强断言**，Rust 实现应保留等价语义。
- 规范化比较工具：`prepareSessionSnapshotFixtureForComparison()`（`:303-312`）把任意代 fixture 就地转成当前物理格式用于期望输出比对；`parseSessionLog`（`:200-209`）接受投影行（无 envelope）并补确定性 dense seq。

### 5.5 Web scaffold（真组合 + 私有世界 + 回环 HTTP）

- 模式由 `DSH_SNAPSHOT` 决定：空/`replay` → `replay`（`scaffold.ts:126-137`）；`test:web:perf:built` 显式设 `DSH_SNAPSHOT=replay`（`package.json:82`）。
- `launchWebScaffold`（`:476+`）要点：
  - 用**发布的 bundle patch 栈**组合出真组合（base + web-app，`:577-583`），只把模型换掉；
  - 临时 workspace / persistence root / `$DSH_HOME` / skill 根全部钉在私有临时目录（`:520-560,605-620`）；
  - 端口由 OS 原子分配（`baseUrl` 真回环服务，`:278-303`）；
  - `session-persistence-jsonl` 根覆盖（`:599`）、`session-query-sqlite` 覆盖为 `{path:':memory:', openAt:'first-search'}`（`:604`）——**搜索 opt-in 只在这个 lane 打开**；
  - `paceMs` 透传给 replay（`:384,897`）。
- `seedSession`（`:1455-1512`）走**真 Session + JSONL API** 写入，且要求 fixture 以 `turn/end` 结尾（`:1466-1468`）；默认 `createdAt = Date.now() - 60_000`（`:1469`），可用 `options.createdAt` 固定——**跨实现比对时必须显式固定**。
- `watchConsole`（`:1773-1776`）收集 `warnings` / `pageErrors`，benchmark 断言两者为空（`long-session.bench.ts:433-434`）。

### 5.6 观测原语（浏览器侧）

| 原语 | 作用 | 证据 |
|---|---|---|
| `rAF × 2` | "渲染机会"，非呈现时间戳 | `long-session.bench.ts:34-37`，规则 `benchmarks/AGENTS.md:8` |
| `waitForFunction(polling:'raf')` | 避开 selector 重试退避 | `:97-106` |
| `Performance.getMetrics` | `TaskDuration` 主线程忙时 | `:62-67` |
| `data-chat-flow-key^="9:turn-tail"` | 翻页就绪的信号（tail 数量增长） | `:15,55` |
| `data-composer-input[contenteditable]` | 输入焦点与见证 | `:89,405-409` |
| `HeapProfiler.collectGarbage` | 浏览器堆基线 | `:421` |

---

## 6. 正确性是怎么被（不）证明的

### 6.1 benchmark 内部只断言"到达端点 + 结构计数"

治理规则明写：**"Do not … turn benchmark completion into duplicate semantic assertions"**（`benchmarks/AGENTS.md:15`），Note 进一步解释"计时 case 不重复功能测试拥有的语义断言，只要求目标调用完成并到达被测端点"（`2026-09-04-session-open-performance-gate.md:38`）。

实际存在的护栏分三类：

| 类别 | 例子 | 证据 |
|---|---|---|
| 结构计数 | list 返回 3,000 行且全部带 projection；fold 的 `chatNodes > 0`；terminal 的 `truncated` 与精确字节数；agent 的 requests/toolCalls 精确等于预期 | `session-corpus.bench.ts:261`、`conversation-fold.bench.client.ts:90`、`terminal-io.worker.ts:79-85`、`agent-continuation.worker.ts:113-116` |
| 端点身份 | `nextFrame==='transient'`、`waitReason==='inferred_idle'`、`items===sessions`、`hasMore` | `reconnect.bench.client.ts:51`、`terminal-io.worker.ts:79`、`projection-list.worker.ts:267-269` |
| 环境洁净 | `pageErrors==[]`、`warnings==[]`、fork 的每个 rank 必须出现 | `long-session.bench.ts:433-434`、`session-corpus.bench.ts:299-303` |

### 6.2 真正的语义锚点在别处

- **迁移语义**：`session-open.bench.ts:352-398`（未计时）断言迁移后事件集合相等、恰好 1 条 system/message、`deriveMessages()` 逐条相等、V0 文件字节**未被修改**、successor 可重开。
- **回放完整性**：`assertConsumed()`（`llm-replay/src/index.ts:171-180`）。
- **两表示等价**：`digest(compactFold) === digest(rawFold)`（`history-transport.perf.client.ts:466`），以及 `compactClientHeap === rawClientHeap`（`:467`）。
- **字节冻结的历史 fixture + 独立的期望输出**：`snapshots/` + `writer.expected.jsonl`（`2026-08-24-session-log-snapshot-corpus.md`，"record and refresh never rewrite an explicitly retained historical fixture"）。
- **工作区终态快照**：`assertFinalWorkspaceSnapshot`（`scaffold.ts:145-155`）——模型自述不能冒充正确性。

### 6.3 跨实现等价性缺口（Rust 重做必须自己补）

现状没有任何"另一实现"的对照；所有 golden 都是"本实现的历史版本"。缺的具体项：

1. **没有 golden digest**：list/fork/search 的 wire JSON 只被计数，没有被序列化后哈希留存。`[INFERENCE]` 这意味着换实现后"字段名、字段顺序、null vs 缺省、时间戳"这类差异不会被现有门禁发现。
2. **没有跨实现 fixture**：所有 fixture 都由 TS 侧生成器现算；虽然字节可复现（固定时钟/固定 id/Zstd 确定性），但没有"冻结一次、双方共用"的目录约定。
3. **没有启动/常驻内存门禁**：`bootMs` 只存在于 session-corpus 的 list case（`session-corpus.worker.ts:294`）与手工 perf（`complex-history.perf.ts:1216-1218`），不进门禁；RSS 只有 `recordPeakMemory`（不缩放、不在 OS 限制下跑）。

---

## 7. 潜在混淆因素（读任何"更快"结论前必须排除）

| # | 因素 | 具体表现 | 证据 |
|---|---|---|---|
| Q1 | **机器尺度** | 预算里乘了 2×（x64 CI / arm64 参考机）。拿 CI 预算去比本地数字会系统性高估/低估 | `calibration.ts:4`；Note `:40` |
| Q2 | **缩放是估计** | PassMark 跨 ISA、突发 vCPU、混合核都"coarse"；`ioLatencyUs` 是"typical"估计值 | `machine-profiles.ts:4-13` |
| Q3 | **小样本** | 多数端点 3 或 5 个样本取 median；session-corpus 的 search 与 longest fork 各 **1** 个样本 | `session-corpus.bench.ts:33,274,298` |
| Q4 | **统计量不一致** | median / best-of-N（取最小）/ min-of-means 三种并存；best-of 会系统性偏乐观 | `conversation-fold.worker.client.ts:168-179`；`user-text.bench.client.ts:112-117` |
| Q5 | **JIT/tiering 波动** | user-text 观测到同一 worker 的 CI 比值达参考的 1.65×，因此单独给 1.75× 余量 | `user-text.bench.client.ts:15-20` |
| Q6 | **GC 时机** | 端点内存 = 双 GC 后 `heapUsed` 差；显式 GC 时间被排除；极限 GC 不进基线 | `session-open.worker.ts:110-117`；Note `:23,36` |
| Q7 | **内存口径混用** | `heapUsed`（V8 堆）与 `maxRSS`（进程常驻，含 native）是两件事；list 场景 JS 堆 ~100 MB 而 RSS 达 0.7–1.2 GB | `2026-10-07-memory-limited-benchmarks.md:36`（Alternatives） |
| Q8 | **"cold"不是冷存储** | 明确写"Filesystem caches are not evicted, so cold means a fresh process, not cold storage" | Note `2026-09-28-session-corpus-performance.md:80` |
| Q9 | **ioShare 是声明值** | session-open 的 `{file:0.1, memory:0}`、浏览器 0.05、projection-list 0 —— 不是测量值 | `session-open.bench.ts:85`、`long-session.bench.ts:27`、`projection-list.bench.ts:22` |
| Q10 | **`rAF×2` ≠ 呈现** | 只证明"有渲染机会"；无 GPU/合成器时间戳 | `long-session.bench.ts:34-37` |
| Q11 | **Playwright actionability** | draft typing 三个样本 124.97–504.96 ms；托管两次运行 932.7 vs 142.1 ms。端点含调度与 actionability，不是按键延迟 | Note `2026-09-06-frontend-performance-budgets.md:39,52,57` |
| Q12 | **浏览器 workflow 非独立冷场景** | open/page/trajectory/first/stream 是**一个串行工作流**的端点，前面的动作会预热后面 | Note `:13`（"These are endpoints of one workflow, not independent cold scenarios"） |
| Q13 | **测试 Host vs 发布 Host** | 浏览器 lane 的 Host 由 Vitest 源码解析器托管，只有 Client 是构建产物；规则要求"把源码解析的测试 Host 证据与发布 Host 证据分开报" | `benchmarks/AGENTS.md:8` |
| Q14 | **Node 侧端点无传输** | corpus/projection-list/open 都是 in-process controller 调用；浏览器侧才是真回环 HTTP | Note `2026-09-28-session-corpus-performance.md:80` |
| Q15 | **语料只有 22 个不同 body** | 1,000/3,000 个 Session 共享 22 个 body → search 索引高度重复文本，可能低估/高估真实索引成本 | Note `:80` |
| Q16 | **语料只有当前代** | 真实语料大多在旧代；list 的 revision hashing 与 migration-on-read 未被覆盖 | Note `:80` |
| Q17 | **search 是 opt-in** | shipped profile 关掉（`openAt: never`），benchmark 才打开 `first-search`；预算文本自己注明"shipped profiles disable it" | `session-corpus.bench.ts:83`、`session-corpus.worker.ts:142-143` |
| Q18 | **手工 perf 无断言** | `complex-history.perf.ts:1-4` 明说没有时间断言；只有结构计数 | 同左 |
| Q19 | **播种成本可能污染** | session-corpus 单文件含播种（~1.5 GB）+ 三个 case，300 s 上限；larger corpus 被否掉正是因为它挤占余量 | Note `:78`；`session-corpus.bench.ts:29` |
| Q20 | **播种并发本身是瓶颈** | 无界 Zstd 压缩峰值 7.7 GB RSS；改为 8 并发；两进程准备各自 59/65 s（共享 2 CPU） | Note `:32,64` |
| Q21 | **`createdAt` 默认用 `Date.now()`** | `seedSession` 默认 `Date.now()-60_000`，事件时间被 rebase → 逐字节比对会失败 | `scaffold.ts:1469,1485-1509` |
| Q22 | **预算被反复重标定** | 浏览器 open 历史 500 → 中间 875 → 期望 900（hosted median 875.3）→ 1125；说明该端点对 runner 敏感 | Note `:57-69`；`long-session.bench.ts:122-135` |

---

## 8. 给 Rust 重做的验证方案（"更快且不更错"）

### 8.1 先给端点分区（否则会拿不可动的指标吹收益）

| 分区 | 端点 | Rust 能否影响 |
|---|---|---|
| **A. Host 计算/IO（主战场）** | session-open 四阶段、first-history、agent-resume、corpus list/search/fork、projection-list、agent-continuation、terminal-io ingest、user-text 扫描、reconnect 的 `replace()`（若下沉到 Rust） | ✅ 直接 |
| **B. 传输/序列化** | wire JSON 字节数、`JSON.stringify` 成本、gzip/brotli | ✅ 可省字节/改编码，但改变协议即改变口径 |
| **C. 浏览器渲染** | fold 时间（若仍在 JS）、rAF、TaskDuration、输入延迟 | ⚠️ 仅通过"少数据/少往返"间接影响；**不得**把它的改善算作 Rust 收益 |
| **D. 未测但有意义** | 进程启动到 ready、常驻 RSS、多会话并发、崩溃恢复、优雅退出 | ❌ 现有门禁没有 → 必须新增 |

### 8.2 E0：冻结输入（一次性，用现有 TS 生成器产出字节）

目标：让 JS 与 Rust 读**同一批字节**，从此不再各自生成。

1. 用 `writeSyntheticReleasedV0Session` 生成 127,400 事件的 V0 目录（`session-open/synthetic-released-v0-session.ts:166-192`），保存 `session.jsonl.zstd` 与其 facts（events/rows/frames/logicalBytes/compressedBytes）。
2. 用一次真实迁移（benchmark 的 `post-upgrade-template` 手法，`session-open.bench.ts:209-215`）生成当前代 successor，**V0 与 successor 一并冻结**。
3. 用 `SyntheticCorpusWriter` 生成 list(3,000)/search(1,000)/fork(1,000) 三个根 + `storages/<projectionCacheDomainSpec>/sessions/*.json`（`session-corpus.worker.ts:256-266`），记录 `fixture.facts` 与全部文件 sha256。
4. 冻结 240-turn（`long-session-browser/synthetic-history.ts:25-87`）与 800-turn（`agent-continuation/workload.ts:66-105`）JSONL。
5. 冻结 `llm-replay` 的 `ReplayEntry[]` 脚本与 override 文档（含 `paceMs`），作为"模型边界"的唯一真源。
6. 冻结 `terminal-io` 的注入模式（chunk 大小、prefill、timed bytes、OSC 形状，`terminal-io.worker.ts:27-96`）与 `user-text` 的全部文档常量（`user-text.bench.client.ts:34-78`）。

> 验收：E0 只需断言"每次重建的 sha256 与清单一致"，即可防止 fixture 漂移。这也是现有 benchmark 缺的东西。

### 8.3 E1：等价性门（正确性，先于性能）

对每个冻结 fixture，Rust 实现必须产出与 JS 基线**逐字节/逐字段**一致的结果。建议的 golden 清单（按可获得性排序）：

| Golden | 生成方式（JS 侧） | 比较方式 |
|---|---|---|
| 迁移后事件流 | `open('read').read().events` | 严格 JSON 相等（时间戳归一后再比） |
| 派生消息 | `Session.fromRestore(...).deriveMessages()` | 严格相等（已有先例：`session-open.bench.ts:372-380`） |
| projection hydrate 结果 | `sessionProjections.hydrate(session, {}, seed, SessionLogOffset(0))` | 严格相等 |
| list 的 wire JSON | `controller.list()` → `JSON.stringify` | sha256（注意 Q21：先固定 `createdAt`） |
| search 结果 | `controller.search({query})` 两次 | 有序命中列表相等 |
| fork 子 header | `controller.fork(...)` 返回的 header | 严格相等（id/createdAt/cwd/parent/delegationDepth/inheritedEventCount） |
| first-history 首页 | `follow()` 的第一个 `snapshot`（records 顺序与 seq） | 严格相等 |
| terminal 端点 | `viewport` / `read.text` 字节 | 字节相等（已有先例：`terminal-io.worker.ts:83-84`） |
| 浏览器可见文本 | 240 turn 的 tail 计数与 `data-chat-flow-key` 序列 | DOM 结构计数 + 文本哈希 |
| 回放消费 | `assertConsumed` 语义 | 每个 script 被完全消费；少一次调用即失败 |

三条硬规则：
1. **V0 字节不可变**（`session-open.bench.ts:390` 的模式）：Rust 只读 migrator 必须不改写源文件。
2. **`createdAt` 固定**：所有跨实现比较都传 `options.createdAt` 或走 `prepareSessionSnapshotFixtureForComparison`（`llm-replay/src/index.ts:303-312`）归一，否则 Q21 会制造假失败。
3. **等价性验证必须包含"未实现也要报错"的路径**：例如对 `openAt: 'never'` 的 search 必须拒绝、扩展 UI pending 必须阻塞切换、approval 必须挂起。这些语义现有 benchmark 不测，只能从功能测试搬（`benchmarks/AGENTS.md:15` 明确不许在 benchmark 里重复，但**可以**作为独立等价性套件）。

### 8.4 E2：同机 A/B 性能（可比较的唯一做法）

协议（每一条都对应一个已存在的坑）：

1. **同机、同 fixture、同端点边界**。Rust 侧不得把 in-process 调用换成本地 socket（换即口径变化，Q14）。
2. **每端点 ≥5 个独立进程样本，取 median**；同时报 min/max/全样本（`benchmarks/AGENTS.md:13`）；Rust 与 JS 交替运行（A,B,B,A…）以抵消热漂移。`[INFERENCE]` 交替运行是本机 A/B 的常规手段，仓库未实现，需自建。
3. **禁用任何环境变量覆盖预算**（`benchmarks/AGENTS.md:13`）。
4. **不要用 `ciTimeBudget` 的 2× 去比较**：2× 是"CI/参考机"换算，不是 A/B 因子（Q1）。A/B 只比"同机 median 比值 + 分布重叠"。
5. **CPU 时间必须一并报**：Rust 用 `getrusage`/`clock_gettime(CLOCK_PROCESS_CPUTIME_ID)` 提供 user+sys，填进 `recordTimings` 的 `cpuMs`，这样 `report.html` 的 `io`/`parallelism`/`coreFactor` 才不会退化成声明值（`scaling-report.ts:203-205`）。
6. **内存分开报**：JS 侧 = 双 GC 后 `heapUsed` 差 + `maxRSS`（`session-open.worker.ts:99-129`）；Rust 侧 = `VmHWM`/`peakRSS` 与稳态 RSS。**不要**把 Rust 的 RSS 直接与 V8 的 `heapUsed` 对比（Q7）。
7. **浏览器端点（C 区）单列**，只用来证明"没有回退"，不进入"更快"的结论。

### 8.5 E3：负控（证明断言真的会红）

现有仓库的做法值得照抄：每个 calibration `describe` 都包含"接受已记录样本 + 拒绝合成回归"的成对控制（`session-open.bench.ts:311-338`、`agent-continuation.bench.ts:84-158`、`long-session.bench.ts:122-150`、`active-stream-reconnect.bench.client.ts:25-38`、`session-corpus.bench.ts:192-205`）。

Rust 侧至少要三条：
- 在目标端点人为插入 2× 延迟 → 预算断言必须失败；
- 故意返回少一行 / 少一个 projection / 少一个 tool call → 结构断言必须失败；
- 故意不消费一个 replay script → `assertConsumed` 必须失败。

### 8.6 E4：新增端点（Rust 的真实卖点，必须自己建）

| 新端点 | 为什么必须新增 | 建议口径 |
|---|---|---|
| `startup.timeToReadyMs` | 现有门禁只测 Host boot（`session-corpus.worker.ts:294`），不测进程/CLI 启动 | 从 `spawn()` 到"第一个请求可服务"（子进程 stdout 的 ready 行），5 样本 median；对手方 JS 用 `apps/cli/lib/bin.js` + 真 web profile（参考 `apps/cli/tests/lazy-search-startup.compat.spec.ts:1-10`） |
| `process.steadyRssMb` / `peakRssMb` | 现有只有 case 级 `recordPeakMemory`，且未在 OS 限制下跑 | 复用 `run-memory-pressure-bench.ts` 的 cgroup 机制（4/8 GiB, swap=0），报 `memory.peak` 与 `oom_kill` |
| `concurrency.throughput` | 现有全部单进程串行 | N 个会话（N=1,2,4）并发跑同一 open 端点，报 p50/p95 与总吞吐；同时报总 RSS |
| `shutdown.gracefulMs` | 现无 | SIGTERM → 进程退出，5 样本 median；断言会话日志已 flush |
| `recovery.crashResumeMs` | 现无 | 写到一半 SIGKILL，重启后 open+resume 端点 |

必须与既有口径对齐的部分：同样"fresh child + private mkdtemp + median + 报全样本"（`benchmarks/AGENTS.md:10,13`）。

### 8.7 判据（什么时候可以对外说"更快"）

同时满足才成立：

1. E1 golden **全绿**（含未实现路径的拒绝语义）；
2. 对 A 区每个端点：同机 median 改善，且 **max 不恶化**（防止"中位数好看、尾部爆掉"）；
3. 对 B 区：wire 字节数不增加；若减少，需重跑 C 区确认没有引入额外往返；
4. 对 C 区：无回退（允许不变）；
5. 内存：稳态 RSS 与 peak RSS 都不高于基线，或在 4/8 GiB cgroup 下都不 OOM 且预算不变；
6. 明确列出**不可比清单**：任何端点边界、统计量、fixture、机器、样本数发生变化，都必须重标定，不能与旧数字并列。

---

## 9. 未验证/假设清单

- `[INFERENCE]` 仓库内的 `.dsh-report/report.html` 与 `results.jsonl` 不在版本库中（`.dsh-report` 被 Git 忽略，`scaling-report.ts:25-28`），因此本文引用的所有"hosted CI 数字"都来自 Agent Note 的文字记录，无法在本机复核原始 run 日志。
- `[INFERENCE]` `session-corpus` 的 "38 cases"（`2026-10-07-memory-limited-benchmarks.md:25`）是 4 个压力文件的 case 总数，而非单文件 case 数；单文件 case 数未逐一核算。
- `[INFERENCE]` Rust 侧能否直接读现成 Zstd Session 日志，取决于 `packages/session/session-persistence-jsonl/src/format.ts` 的帧分组与 envelope 细节；本文只确认了 benchmark 生成端使用 `compressZstdFrame` 与 `eventLines`（`synthetic-corpus.ts:10-15,272-282`），未审计解码端契约。
- `[INFERENCE]` "增量 delta 回放比 record-proportional fold 慢 ~11×"（`conversation-fold.bench.client.ts:44-46`）来自库内注释的既有观测，不是本次复现。
- 未执行：任何 `pnpm`/`vitest`/`tsdown`/benchmark 命令。第 10 节的命令仅作记录。

---

## 10. 命令记录（未执行，仅供后续 E0–E4 使用）

```bash
# 构建（库 + workers + web）
pnpm run build:bench          # package.json:24
pnpm run test:bench           # package.json:65 = build:bench + build:web + test:bench:built
pnpm exec vitest run --config vitest.bench.config.ts benchmarks/session-open   # 单目录
pnpm run test:bench:built     # package.json:66

# 内存压力（Linux only）
pnpm run test:bench:memory 4g # package.json:67 → scripts/run-memory-pressure-bench.ts
pnpm run test:bench:memory -- --scoped 8g

# 手工诊断（无时间断言）
pnpm run test:web:perf        # package.json:81 = build + DSH_SNAPSHOT=replay + vitest.web.perf.config.ts

# CI 门禁
pnpm run check:ci:bench       # package.json:94 → scripts/run-gates.ts ci-bench → run-ci-bench.ts
```

---

## 附：一句话索引（按问题查）

- 想知道"预算怎么来的" → §3.4 + `benchmarks/support/calibration.ts:13-15`
- 想知道"为什么必须编译产物跑" → `benchmarks/AGENTS.md:7` + `2026-09-04-session-open-performance-gate.md:109` + `built-worker.ts:33-38,88-101`
- 想知道"哪些数字不能跨机比较" → §7 Q1/Q2/Q3/Q22 + `machine-profiles.ts:4-13`
- 想复用输入 → §5.1/§5.2/§5.4/§5.5
- 想证明"更快" → §8.4；想证明"没更错" → §8.3
- 想找"最像 Rust 收益点"的现有证据 → `session-corpus.bench.ts:45-54`（search 首建 144 s、fork 最长 20 s、list 3k 首次 3.2 s）
