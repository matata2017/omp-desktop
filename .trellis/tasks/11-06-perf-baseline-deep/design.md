# 设计: 发布版性能基线与深度测量

## 1. 边界

三层，互不越权：

1. 测量器：`scripts/perf/measure.ps1`（进程与内存）加 `scripts/perf/drive.mjs`（CDP 驱动 UI）。只读应用状态，不改应用。
2. 应用内计时点：Rust 侧 `tracing` boot 标记，renderer 侧 `performance.mark`。只加标记，不改行为。
3. 对照物：`.research/omp-gui-git` 的上游 Electron 产物。只用于同口径比较。

## 2. 单测量器契约

```
scripts/perf/measure.ps1 -Exe <path> -Label <name>
  [-Tabs N] [-Drive] [-Port 9222]
  [-ReadyTimeout 30] [-Settle 15] [-Samples 10]
  [-LogPath <path>] [-Out <json>]
```

行为：

- 清理只针对 `-Exe` 对应的进程（进程名或可执行路径），不清扫整个 checkout。
- `-Drive` 时设置 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=<Port>`，退出时恢复原值。
- 冷启：`Start-Process` 到进程树里任一窗口句柄非零。
- 稳态：轮询 `runtime.log` 出现 `sidecar ready`，超时记 `readyTimedOut=true` 并继续采样，不静默。
- 采样：ready 后再等 500 ms，采 10 次、间隔 250 ms，输出中位数与 min/max。
- 进程树：从启动 pid 递归 `ParentProcessId`。角色按命令行分 `stats`、`sidecar`、`webview2`、`crashpad`、`app`、`helper`。
- 输出：stdout 人类可读表加 `-Out` 指定 JSON。退出码 0 成功，2 无窗口，3 ready 超时，4 exe 不存在。

JSON 形状（字段稳定，gate 依赖）：

```json
{
  "label": "release-idle",
  "exe": "romp-desktop.exe",
  "exePath": "D:\\rust-omp\\target\\release\\romp-desktop.exe",
  "builtAt": "2026-11-05T21:57:00",
  "startedAt": "2026-11-06T02:30:00+08:00",
  "coldStartMs": 210,
  "milestones": {
    "windowMs": 210,
    "webviewMs": 480,
    "sidecarReadyMs": 2600,
    "firstSessionMs": 3400
  },
  "treeRss": { "medianMB": 640.5, "minMB": 610.2, "maxMB": 700.1 },
  "processes": 9,
  "handles": 1200,
  "threads": 90,
  "breakdown": [
    { "role": "webview2", "mb": 420.0, "processes": 5, "handles": 800, "threads": 60 }
  ],
  "readyTimedOut": false,
  "tabs": 1
}
```

`-Tabs` 必须真的开 tab：调用 `drive.mjs` 走 `window.omp` 的 tab API，开完等每个 tab 的 sidecar ready 或超时，再采样。只回显数字的实现直接删除。

## 3. 启动时间对齐

冷启由 harness 的 `Stopwatch` 给出，基准是 `Start-Process` 调用时刻，单位 ms。

Rust 标记写进同一个 `runtime.log`，用默认 `tracing_subscriber::fmt` 时间戳。新增：

- `boot process_start`：`main` 进入后第一行。
- `boot window_visible`：窗口创建并显示后。
- `sidecar ready`：已有，补 `target = "boot"` 或保留原文由 harness 匹配。

renderer 标记用 `performance.mark`：

- `boot:first-paint`：`main.tsx` 首次提交后。
- `chat:first-session-rendered`：`ChatStream.tsx` 历史消息应用完成。

换算：`epoch = performance.timeOrigin + mark.startTime`。`drive.mjs` 通过 CDP 读 `performance.getEntriesByType("mark")` 与 `Date.now() - performance.now()`，与 harness 的 epoch 对齐。四段派生：

- `windowMs = coldStartMs`
- `webviewMs = firstPaintEpoch - startEpoch`
- `sidecarReadyMs = readyEpoch - startEpoch`
- `firstSessionMs = max(firstPaintEpoch, readyEpoch) 后的 firstSessionEpoch - startEpoch`

任何一段缺失就写 `null`，不许用 0 冒充。

## 4. 上游对照

源在 `.research/omp-gui-git`，与 `replica/recon.md` 记录的 v0.9.35 对齐。构建命令与日志落到 `scripts/perf/upstream-build.md`（命令、版本、错误）。

同 `measure.ps1`，但 Electron 没有 `sidecar ready` 行，用 `-NoReady` 加固定 settle。对照表必须标注两边的就绪判据不同，不把差异藏进口径。

构建失败时：保留错误日志路径，对照降级为发布版多次自比，PRD 的未决项保持打开。

## 5. 进程模型实验

事实：每个 tab 一个 `omp.exe --mode rpc-ui`；RPC 命令不带 sessionId，进程内只有一个活动会话；同 workspace 切会话用 `switch_session`。

假设 H1：共享一个 sidecar，命令前切会话，可以服务多个 tab。

探针 `scripts/perf/probe-shared.mjs` 直接对一个 sidecar 的 stdio 发命令序列：tab A `prompt` 流式未结束时发 tab B `switch_session` 加 `prompt`，观察 A 的流是否继续、事件是否串台。

三组测量：

1. per-tab idle：2 个 tab 各一个进程。
2. per-tab streaming：2 个 tab 同时流式。
3. shared probe：一个进程，交错命令。

停止条件：A 的流被中断或事件无法按 tab 归属，判 H1 否决，不写共享实现，只在 `docs/architecture.md` 记录原因和数字。若 H1 成立，另开任务，不在本任务里改架构。

## 6. 流式渲染

不改 `markdown.tsx`。用 CDP：

- `Profiler.enable` / `Profiler.start`，发一条真实 prompt，`agent_end` 后 `Profiler.stop`。
- `Performance.enable`，记录 `TaskDuration`、`ScriptDuration`、`LayoutCount`、`RecalcStyleCount`、`JSHeapUsedSize` 的前后差。
- 汇总 profile 的 self-time top 10。

只有当热点落在 `ChatStream.tsx` 才改代码，且只改一处，改动前后跑同一脚本。热点在上游文件时，报告结论，不动上游。

## 7. UI 审计

`scripts/ui/audit.mjs`：

- 扫 `en.ts`、`zh.ts`、`ru.ts`：破折号、弯引号、unslop 词表。按 `t("key")` 在 `src/renderer` 的使用次数排序。
- 通过 CDP 注入 axe-core（devDependency，MPL-2.0），对当前窗口跑 `axe.run()`，输出 violations 的 id、impact、节点数。
- 截图用 CDP `Page.captureScreenshot`，避免后台窗口 PrintWindow 空白。

改文案只取使用频次最高的用户可见键；每处登记进 `docs/handoff.md`。不改语义，不改布局。

## 8. 预算门

`scripts/perf/gate.ps1 -Baseline <json> -Current <json> [-MaxRegressionPct 10] [-MaxRssMB <n>]`：

- 缺字段返回 2。
- `coldStartMs` 或 `treeRss.medianMB` 相对基线超过阈值返回 1。
- `readyTimedOut=true` 返回 1。
- 预算值在发布版基线出来后填入 `docs/perf.md`，脚本接受显式参数，不内置猜测。

## 9. 风险与回滚

- 发布版构建约十几分钟，且有 NSIS 下载失败的历史。先测已有 release 二进制，代码改动后重建一次。
- 上游 Electron 构建可能因网络失败。失败留证据，不阻塞其余切片。
- 冷启受 OS 缓存影响。每次运行前不保证清缓存，用 3 次中位数并记录方差。
- boot 标记在启动路径上，必须无分配、无 I/O；`performance.mark` 只在开发和测量构建里跑。
- 回滚：测量器与文档独立；boot 标记只是日志；gate 不参与启动。
