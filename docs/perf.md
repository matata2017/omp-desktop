# 性能实测

测量日期：2026-11-06（发布版基线），2026-11-08（懒启动 stats 后复测）。测量器：`scripts/perf/measure.ps1`（进程/内存），`scripts/perf/drive.mjs`（CDP 驱动与 renderer 标记），`scripts/perf/gate.ps1`（预算门）。原始 JSON 在 `.perf/`（已忽略，不入库）。

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

`-Tabs 1` 只测应用自己启动的那一个 active tab 的 sidecar。`drive.mjs` 会先等应用把 active tab 拉到 ready，再决定是否激活额外 tab；抢在应用之前 `setActive` 会多起一个 sidecar，曾把 1 tab 测成两个。

## 启动分段（release，1 tab）

| 里程碑 | 实测 |
|---|---|
| 进程启动 → 窗口可见 | **327 ms**（多次 250–655 ms） |
| → renderer 首帧（`boot:first-paint`） | **1861 ms**（多次 1.7–4.6 s） |
| → sidecar ready | **4013 ms** |
| → 首会话可交互（`chat:first-session-rendered`） | **4063 ms** |

窗口本身不是瓶颈；WebView2 初始化占掉首帧前约 1.5 s，sidecar 启动再占约 2.2 s。用户感知的“打开就到能用”是约 4 s，不是 300 ms。首帧与冷启的方差较大，单次结果不可当预算。

## 内存足迹（release，懒启动 stats 后）

| 状态 | 进程树中位数 | sidecar | WebView2 | app | 进程数 |
|---|---:|---:|---:|---:|---:|
| 1 tab 空闲 | **712.0 MB** | 231.4 MB / 1 | 448.9 MB / 6 | 34.5 MB | 9 |
| 2 tab 空闲 | **978.7 MB** | 529.0 MB / 2 | 442.6 MB / 6 | 34.7 MB | 11 |
| 4 tab 空闲 | **1442.7 MB** | 928.9 MB / 4 | 441.5 MB / 6 | 34.6 MB | 15 |

每多一个 tab 约 **+240 MB**（(1442.7 − 712) / 3 = 243.6 MB），全部来自 sidecar。app 与 WebView2 是固定开销，约 480 MB。

改懒启动之前，同样的构建是 840.8 / 1075.9 / 1553.5 MB，每一行都多一个常驻 97 MB 的 `omp stats` 子进程。dev 构建同口径是 883–894 MB（1 tab）与 1273 MB（4 tab）。原计划的 ①120 MB、②250 MB、③500 MB 全部不成立：只要常驻一个 agent 进程，1 tab 就已经 712 MB。可执行的表述是“固定开销约 480 MB + 每 tab 约 240 MB”。

## stats 改为懒启动

原来 setup 里就 `ensure_running()`，空闲常驻 97 MB，而多数会话根本不打开统计页。改法是把启动挪到真正需要的地方：

- `ipc.rs` 的 `stats:url` 改为 `await state.stats.ensure_running()`。`stats:fetch` 本来就会懒启动。
- `lib.rs` 的 setup 删掉预启动 spawn，退出时的 `terminate_now()` 保留。

只删 spawn 是不够的：命令面板的 `/trace` 先调 `window.omp.stats.url()`，而 `use-stats.ts` 只在统计组件挂载时才 `stats:fetch`。若 `stats:url` 仍在 port 为 None 时返回 `unavailable:true`，不打开统计页就用 `/trace` 会一直弹“未就绪”。

实测（干净进程表，最终 release）：

| 步骤 | 本仓库 `omp.exe stats` 子进程数 |
|---|---:|
| 启动后、调用 `stats:url()` 前 | **0** |
| CDP 调 `await window.omp.stats.url()` | 返回 `http://127.0.0.1:54789/` |
| 调用后 | **1** |

即省下 97 MB 常驻，`/trace` 与统计页仍按需拉起服务。

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
powershell -File scripts/perf/gate.ps1 -Baseline .perf/release-idle.json -Current .perf/release-idle.json
```

缺字段返回 2；`readyTimedOut=true`、冷启或内存相对基线超过 `-MaxRegressionPct`（默认 10%）返回 1。已用基线自比（exit 0）与人为加倍（exit 1）验证两条路径。预算值等下一次发布版基线确定后写进 `-MaxRssMB` 调用，不内置猜测。

## 已知问题

- **强杀仍留孤儿**：`taskkill /T` 只在测量结束时清理；用户强杀主进程时 sidecar/stats 会存活。实测 `-KeepRunning` 后按名字杀主进程，会留下一个 `omp.exe stats` 孤儿，需要按可执行路径额外清理。要彻底覆盖需要 Windows Job Object。
- **sidecar ready 约 4 s**：启动关键路径的最大一段，下一步单独测（sidecar 自身的 `spawn → ready` 与首次模型/工具初始化）。
- **首帧方差 1.7–4.6 s**：WebView2 初始化占主因；区分 environment 创建与页面加载需要 ETW 轨迹。
- **冷启方差 250–655 ms**：受 OS 缓存与 WebView2 版本影响；脚本取中位数，单次结果不可当预算。
