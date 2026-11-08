# 性能实测

测量日期：2026-11-06（发布版基线），2026-11-08（懒启动 stats 后复测，drive 修复后重测）。测量器：`scripts/perf/measure.ps1`（进程/内存），`scripts/perf/drive.mjs`（CDP 驱动与 renderer 标记），`scripts/perf/gate.ps1`（预算门）。原始 JSON 在 `.perf/`（已忽略，不入库）。

## 先看构建类型

`cargo build --release` **不是**发布版。它仍指向 `devUrl`（`http://localhost:1420`），Vite 一停，窗口在 260 ms 出现，但 renderer 永远不加载：`window.omp` 不出现，sidecar 不启动，进程树只有 460 MB 的空壳。发布版必须用 `bunx tauri build --no-bundle`，它会跑 `build:web` 并把 `dist` 嵌进自定义协议。

本文所有发布版数字都来自 `bunx tauri build --no-bundle` 的产物（`target/release/romp-desktop.exe`）。

## 方法

1. 只清理目标 exe 对应的进程；同一 app 标识的 debug/release 会互相顶掉单实例锁，因此 `romp-desktop.exe` 按名字清理，其他目标按完整路径清理。
2. `Start-Process` 到进程树里任一窗口句柄非零，记冷启。
3. 开 CDP（`--remote-debugging-port`），`drive.mjs` 等到 `window.omp` 可用，按 `-Tabs N` 启动 N 个 tab 的 sidecar，并读 renderer 的 `performance` 标记。
4. 等 `runtime.log` 出现 N 条本轮 `sidecar ready`（按 `epoch_ms` 过滤旧行），超时记 `readyTimedOut`。
5. 采样 10 次、间隔 250 ms，按进程树（`ParentProcessId` 递归）求中位数。
6. 结束时 `taskkill /T` 清理本次启动的整棵树；`-KeepRunning` 可保留现场。

`-Tabs 1` 只测应用自己启动的那一个 active tab 的 sidecar。`drive.mjs` 会先等应用把 active tab 拉到 ready（用完整的 `-ReadyTimeoutSeconds` 预算，等 active tab 自身状态，不设固定短上限），再决定是否激活额外 tab；抢在应用之前 `setActive` 会多起一个 sidecar。曾有一次坏测量（cold 655 ms、首帧 4597 ms、2 个 sidecar、942.3 MB）就是这么来的，已排除，不进任何基线或方差。

## 启动分段（release，1 tab）

| 里程碑 | 实测 |
|---|---|
| 进程启动 → 窗口可见 | **327 ms**（干净批次 275–694 ms） |
| → renderer 首帧（`boot:first-paint`） | **1861 ms**（干净批次 1724–2763 ms） |
| → sidecar ready | **4013 ms**（干净批次 3783–5357 ms） |
| → 首会话可交互（`chat:first-session-rendered`） | **4063 ms**（干净批次 3838–5420 ms） |

窗口本身不是瓶颈；WebView2 初始化占掉首帧前约 1.4 s，sidecar 启动再占约 2.2 s。用户感知的“打开就到能用”是约 4 s，不是 300 ms。方差来自 OS 缓存与 WebView2 初始化，单次结果不可当预算。

## 内存足迹（release，懒启动 stats 后）

| 状态 | 进程树中位数 | sidecar | helper | WebView2 | app | 进程数 |
|---|---:|---:|---:|---:|---:|---:|
| 1 tab 空闲 | **712.0 MB** | 231.4 MB / 1 | 10.8 MB / 1 | 448.9 MB / 6 | 34.5 MB | 9 |
| 2 tab 空闲 | **978.7 MB** | 529.0 MB / 2 | 21.6 MB / 2 | 442.6 MB / 6 | 34.7 MB | 11 |
| 4 tab 空闲 | **1442.7 MB** | 928.9 MB / 4 | 43.2 MB / 4 | 441.5 MB / 6 | 34.6 MB | 15 |

每多一个 tab 约 **+244 MB**（(1442.7 − 712) / 3 = 243.6 MB），其中约 231 MB 是 sidecar、约 11 MB 是 helper（helper 随 tab 精确线性：10.8 / 21.6 / 43.2 MB）。app 与 WebView2 是固定开销，约 480 MB。以上均为 **WorkingSet 口径**；2026-11-08 安装版 Private 实测：每 sidecar 约 490–540 MB（N=3），固定部分约 388 MB（单样本待复核），两套口径不可相加。

2 tab 那行的 sidecar 为 264.5 MB/个，明显高于 1 tab 的 231.4 和 4 tab 的 232.2，属于跨次采样异常；978.7 MB 含这个偏高样本，边际估算以 1→4 tab 的差值为准。

改懒启动之前，同样的构建是 840.8 / 1075.9 / 1553.5 MB，旧 breakdown 里每一行都有一个常驻 **97 MB** 的 `omp stats` 子进程。新旧同口径差值是 **128.8 / 97.2 / 110.8 MB**，不全是 stats，还叠了跨次测量方差。dev 构建同口径是 883–894 MB（1 tab）与 1273 MB（4 tab），**均为改懒启动之前的旧构建、同样含预启动 stats**，只做结构相对比较。

原计划的 ①120 MB、②250 MB、③500 MB 全部不成立：只要常驻一个 agent 进程，1 tab 就已经 712 MB。可执行的表述是“固定开销约 480 MB + 每 tab 约 240 MB”（**WorkingSet 口径**；Private 口径见上文与 11-08 任务）。

## stats 改为懒启动

原来 setup 里就 `ensure_running()`，空闲常驻 97 MB，而多数会话根本不打开统计页。改法：

- `ipc.rs` 的 `stats:url` 改为 `await state.stats.ensure_running()`。`stats:fetch` 本来就会懒启动。
- `lib.rs` 的 setup 删掉预启动 spawn，退出时的 `terminate_now()` 保留。
- `omp-stats` 的 `ensure_running` 用 `tokio::sync::Mutex` 串行化：懒启动后，打开统计面板时 dashboard 的 sync 与当前路由的 `useStats` 会并发打两个 `stats:fetch`，无互斥时两者都见 `port() == None`、都 spawn，后者覆盖 child slot 并借 `kill_on_drop` 杀掉前者，调用方可能拿到已死进程的端口。
- `stats:fetch` 的失败改为 **resolve** `{ error, unavailable }`，与上游 `main/ipc.ts` 和渲染层 `stats-sync.ts` / `use-stats.ts` 对齐：启动/超时返回 `unavailable: true`（走 90 s 快速重试），已到服务器的请求失败返回 `unavailable: false`（直接显示错误）。之前用 `?` reject 会绕过渲染层的重试预算，首次懒启动失败就弹死路错误。

只删 setup 的 spawn 是不够的：命令面板的 `/trace` 先调 `window.omp.stats.url()`，而 `use-stats.ts` 只在统计组件挂载时才 `stats:fetch`。若 `stats:url` 仍在 port 为 None 时返回 `unavailable:true`，不打开统计页就用 `/trace` 会一直弹“未就绪”。

实测（干净进程表，最终 release）：

| 步骤 | 本仓库 `omp.exe stats` 子进程数 |
|---|---:|
| 启动后、调用 `stats:url()` 前 | **0** |
| CDP 调 `await window.omp.stats.url()` | 返回 `http://127.0.0.1:54789/` |
| 调用后 | **1** |

首次 `/trace` 实测：`stats.url()` 首启 **312 ms**、二次 **7 ms**（CDP 计时，先杀掉 stats 子进程强制重新懒启动）。`openSessionTrace` 在 await 前没有 loading 提示，端口等待上限 15 s。并发两个 `stats:fetch` 实测都返回真实数据（`/api/stats/overview`、`/api/status`）、无 reject、无 `unavailable`，且只 spawn 1 个子进程（互斥生效）——这是打开统计面板时 sync 与 route 并发首用的路径。

## 上游 Electron 对照

来源：`.research/omp-gui-git`（`@oh-my-pi/omp-gui` 0.9.35，MIT），`bun install` + `bun run build`，以 `electron . --user-data-dir=<独立 profile>` 运行，`OMP_BUNDLED_OMP` 指向本仓库 `resources/omp.exe`。未打包，属于源码 dev 运行。

| 指标 | 上游 Electron（dev 源码） | Romp（release） |
|---|---:|---:|
| 冷启到窗口 | 3218 ms | 327 ms |
| 进程树空闲内存 | 1082.9 MB | 712.0 MB |
| 主进程 | 160.5 MB | 34.5 MB |
| sidecar | 463.5 MB / 2 | 231.4 MB / 1 |
| renderer/WebView | 329.1 MB / 3 | 448.9 MB / 6 |

口径差异必须一起读：上游是未打包的 Electron dev 运行，且探针期间开了 2 个 sidecar；Romp 是打包后的 release，1 个 sidecar。冷启差距（约 10 倍）足以定性，内存差距只能作方向参考。上游若要公平对比，需要 `electron-builder --dir` 的打包产物，本机未做（`electron-builder` 未安装）。

## 流式渲染 profile

`scripts/perf/profile-stream.mjs` 对真实 prompt（40 行数字）做 CDP `Profiler` 录制，并用 `Performance.getMetrics` 取前后差：

| 指标 | 实测 |
|---|---:|
| 墙钟时长 | 5791 ms |
| `TaskDuration` | 355 ms |
| `ScriptDuration` | 93 ms |
| `LayoutCount` | 3 |
| `RecalcStyleCount` | 10 |
| `JSHeapUsedSize` | -1.76 MB |

self-time 前三是 idle（5.5 s）、`(program)`（124 ms）、GC（72 ms）。40 行流式回答的渲染成本不到 0.4 s，主线程大部分时间在等模型。结论是当前没有可改的渲染热点；先别动 `ChatStream`，更长的 transcript 再测。

## 共享 sidecar 实验（否决）

问题：每个 tab 一个 `omp.exe --mode rpc-ui` 是内存主变量，能否一个进程服务多个 tab？

探针 `scripts/perf/probe-shared.mjs` 对一个 sidecar 发命令：切到会话 A，`prompt` 开始流式，收到 `agent_start` 后切到会话 B，再切回 A。结果：

- `switch_session` 两次都返回 `success:true`、`data.cancelled:false`。
- 切换后 A 的 `message_update` 归零，`agent_end` 始终没有出现。
- 切回 A 后仍然没有 `message_update`。
- 事件流里没有 session 归属字段，RPC 命令也不带 sessionId。

结论：一个进程只有一个可见流。切换会让在途回答脱离观察者，且无法按 tab 归属事件。共享进程需要改协议（每命令带 session、每事件带归属），不属于本任务范围。决定记在 `docs/architecture.md`。

## 预算门

```
powershell -File scripts/perf/gate.ps1 -Baseline docs/perf-baseline.json -Current .perf/release-idle.json -MaxRssMB 780 -MaxColdStartMs 1500
# 同样的默认调用：bun run perf:gate（先跑 measure.ps1 生成 .perf/ 下的 current JSON）
```

`measure.ps1` 退出码：`0` 成功、`3` ready 超时；exe 不存在或 60 s 内无窗口是 `throw`（-File 下退出码 1）。`gate.ps1` 退出码：`0` 通过、`1` 超预算或 ready 超时、`2` JSON 缺字段。

缺字段返回 2；`readyTimedOut=true`、或内存超预算返回 1。冷启噪声大（275–694 ms），传了 `-MaxColdStartMs` 时 gate 用这个绝对阈值、跳过紧相对比较；内存稳定，同时用相对 `-MaxRegressionPct`（默认 10%）与绝对 `-MaxRssMB`。已用基线自比（exit 0）与人为加倍（exit 1）验证两条路径。`docs/perf-baseline.json` 是入库的便携基线（只含 `coldStartMs` / `treeRss.medianMB` / `readyTimedOut`，不含机器路径与时间戳），checkout 后门就能用；绝对预算 780 MB / 1500 ms，`-Current` 指向本次 `measure.ps1` 的产物。

## 已知问题

- **强杀仍留孤儿**：`taskkill /T` 只在测量结束时清理；用户强杀主进程时 sidecar/stats 会存活。实测 `-KeepRunning` 后按名字杀主进程，会留下一个 `omp.exe stats` 孤儿，需要按可执行路径额外清理。要彻底覆盖需要 Windows Job Object。
- **sidecar ready 约 4 s**：启动关键路径的最大一段，下一步单独测（sidecar 自身的 `spawn → ready` 与首次模型/工具初始化）。
- **首帧 1.7–2.8 s**：WebView2 初始化占主因；区分 environment 创建与页面加载需要 ETW 轨迹。
- **冷启方差 275–694 ms**（干净批次）：受 OS 缓存与 WebView2 版本影响；694 / 2763 / 5357 / 5420 ms 那一批次出现在刚重编完 release、系统缓存冷的时候，进程树仍是 1 sidecar / 9 进程 / 711.8 MB，属于真实方差而非坏测量。脚本取中位数，单次结果不可当预算。
- **首次 `/trace` 有 spawn 等待**：实测首启 312 ms / 二次 7 ms（上限 15 s），`openSessionTrace` 无 loading 提示。

## 2026-11-08：sidecar cap 与 fresh tab（P0）

方法：隔离 `OMP_DATA_DIR` + `PI_CODING_AGENT_DIR`；1 个 fresh active tab，短调用 CDP 逐次 `tab:spawn` 到 4，激活后稳定 8 s 再采样；`ROMP_MAX_SIDECARS` 显式设为 3 / 6。

| cap | spawn 序列 | sidecars | total private | total RSS | 结果 |
|---|---|---|---|---|---|
| 3 | 1→2 2→3 3→3 | 3 | 1828.8 MB | 1219.7 MB | 第 4 个 fresh tab 被拒（`tab:spawn` 返回 null） |
| 6 | 1→2 2→3 3→4 | 4 | 2327.6 MB | 1455.5 MB | 4 个 tab 全部 ready |

原因（源码）：`evict_idle_sidecar` 只回收已绑定 session 的空闲 tab；fresh tab 没有 session 文件，不可驱逐，所以 cap < tab 数时第 4 次 spawn 直接被拒。第 4 个 tab 的代价约 **+499 MB private / +236 MB RSS**（单轮）。

session-bound 对照（N=3，无 `tabs.list` 轮询）。隔离根必须复制 `models.yml`/`models.db*`/`config.yml`/`agent.db*`，否则 resume 报 `SessionModelRestoreError`；每 tab 只 `setActive` 一次，固定 settle 后取两次快照。轮询 `tabs.list` 会走 `tab:get-all → ensure_tab_started`，把一次退出放大成风暴（32 spawns / 26 exits）。

| cap | sidecars | total private（3 次） | 中位 | all-ready |
|---|---|---|---|---|
| 3 | 3 | 1797.2 / 1806.8 / 1784.8 MB | 1797.2 MB | 否：1 个 tab 停在 `starting` |
| 4 | 4 | 2358.6 / 2347.3 / 2346.1 MB | 2347.3 MB | 是 |
| 6 | 4 | 2352.7 / 2366.0 / 2328.3 MB | 2352.7 MB | 是 |

结论：cap3 省约 **550 MB**，代价是第 4 个 tab 不 ready；4 tab 下 cap4 与 cap6 无内存差异。默认 6 保留（给 >4 tab 留余量），文档已对齐。

复现：`powershell -NoProfile -File scripts/perf/cap-matrix.ps1 -Runs 3`（cap 矩阵测量使用 22:00 构建）。

## 2026-11-09：启动分段（安装版 00:51，N=10）

方法：隔离 `OMP_DATA_DIR` + `PI_CODING_AGENT_DIR`（含注册表），1 个 fresh tab；`measure.ps1 -NoDrive` 拿窗口与 sidecar ready，一次 `cdp-eval` 读取 `performance` marks 换算首帧与首会话。复现：`powershell -NoProfile -File scripts/perf/startup.ps1 -Runs 10`。

| 里程碑 | median | p95 |
|---|---|---|
| 窗口可见 | 306 ms | 314 ms |
| first paint | 1591 ms | 1618 ms |
| sidecar ready | 3830 ms | 4127 ms |
| first session rendered | 3868 ms | 4148 ms |

窗口→首帧约 1.29 s（WebView2 初始化 + JS boot）；首帧→sidecar ready 约 2.24 s。第 2 个样本 private 2.1 GB 是残留 sidecar 离群，时间指标未受影响。00:30 构建的 N=10 中位为 321 / 1597 / 3828 / 3877 ms，ghost 修复未回归启动。

## 2026-11-09：100 次 tab open/close（裸 IPC 路径）

方法：隔离根，`cap 6`，每批 `tabs.spawn` + `tabs.close` 各 10 次，共 10 批；每批后采样进程树。**这是裸 IPC 路径**：正常 UI 的 `closeTab` 会先 `switchTab(neighbor)` 再 close，因此该 ghost 复活只在绕过 renderer store 的路径可复现；修复属于防御性加固，不是用户可见泄漏修复。复现：`scripts/perf/lifecycle.ps1 -RawClose`；真实关闭路径用 `scripts/perf/lifecycle.ps1`（默认先 `setActive` 到 base tab）。

| 指标 | raw-IPC（修复前 00:30） | raw-IPC（修复后 00:51） |
|---|---|---|
| sidecars | 1 → **5** | 1 → **1** |
| total private | +1828.2 MB | **−20.7 MB**（811.2 → 790.5） |
| total RSS | +949.3 MB | +2.5 MB |
| handles | +1480 | +68 |
| threads | +229 | +1 |

根因：`tab:close` 不重置 `active_tab`；`tab:get-all` 无条件对 active 调 `ensure_tab_started`，而 `ensure_tab_started` 不校验 tab 是否仍注册 → 已关闭的 active 被复活成 ghost sidecar，每批 +1（约 500 MB/个）。

修复：`tab:close` 在关闭 active 时把 active 切到剩余的第一个 tab；`ensure_tab_started` 对未注册 tab 直接返回；watcher 用 pid 比对忽略被替换的旧进程。00:51 构建复测 raw 路径 100 次：sidecar 恒 1，private 不增长。

## 2026-11-09：WebView2 `--disable-gpu` A/B（N=3，视觉验收）

方法：同一安装版、隔离 `OMP_DATA_DIR` + `PI_CODING_AGENT_DIR`，1 个 fresh tab；`measure.ps1` 新增 `-BrowserArgs` 传入 WebView2 开关；每轮取进程树 + CDP 检查 + 窗口截图。复现：`powershell -NoProfile -File scripts/perf/gpu-ab.ps1 -Runs 3`（需要桌面会话，会按名清理并重启 app）。

| 指标（中位） | baseline | `--disable-gpu` | 差值 |
|---|---|---|---|
| 进程树 Private | 869.5 MB | **748.1 MB** | **−121.4 MB** |
| 进程树 RSS | 743.0 MB | 724.4 MB | −18.6 MB |
| WebView2 Private | 318.9 MB | 203.5 MB | −115.4 MB |
| GPU 进程 Private | 144.8 MB | 30.1 MB | −114.7 MB |
| 窗口可见 | 288 ms | 290 ms | 无回归 |
| sidecar ready | 3778 ms | 3823 ms | 无回归 |

视觉验收：两臂 `document.querySelector('#root').children.length=1`、DOM 节点 391；`.perf/gpu-ab/shots/baseline-1.png` 与 `disable-gpu-1.png` 逐项一致（侧栏、会话卡、字体、图标）。静态 UI 无回归。

**不默认启用**：未验证流式输出、滚动、动画、WebGL/KaTeX/mermaid 渲染；软件渲染可能在负载下变慢。`--disable-gpu` 只作为可选开关，用户可在启动时设 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--disable-gpu`（`measure.ps1` 的 `-BrowserArgs` 用于 A/B）。
