# Design: sidecar 成本、上限与生命周期

## 1. 测量口径

- **单一口径原则**：任何一次比较只用一种口径。Private（`PrivateMemorySize64`）为主；WorkingSet（`WorkingSet64`）只用于相对比较，且必须与 Private 同时给出。禁止把 480 MB（WorkingSet 固定开销）与 500 MB（Private sidecar）写进同一个预算公式。
- 已测参考（2026-11-08，安装版，隔离 `OMP_DATA_DIR`）：
  - 单 sidecar private：空会话 489.2–505.5 MB（N=3，中位 501.0）；恢复 6.92 MB session 534.2–541.8 MB（N=3，中位 537.2）。
  - 同一时刻 total private 944.1 MB − sidecar 555.8 MB ≈ **388 MB 固定部分（单样本，待复核）**。
  - 同刻 total RSS 801.5 MB，与 Private 差 143 MB（共享页）。
- 进程归属：从 `romp-desktop` 根 PID 递归 `ParentProcessId`；role 沿用 `Role-Of`，WebView2 再按 `--type` 细分。
- 采样点：`sidecar ready` 后 `SettleSeconds`，再连续 N 次；每次记录进程数/句柄/线程。
- 分布：每条件 N≥3（首轮），median + min/max；A/B 随机顺序抵消热漂移。

## 2. instrument 改动（`scripts/perf/measure.ps1`）

1. `breakdown` 增加 `privateMB`（role 与逐进程），WebView2 增加 `type` 字段。
2. `-KeepRunning` 时输出 launched PID，便于外层二次采样；文档注明“不带 `-KeepRunning` 会在 finally 杀树”。
3. `Tabs > 3` 注入 `ROMP_MAX_SIDECARS=$Tabs` 时打印 stderr 警告，并回显实际生效值。
4. BOM-free layout helper：`[System.IO.File]::WriteAllText(path, json, UTF8Encoding($false))`；resume 探针以日志 `resume=Some` 为通过条件。
5. 兼容：保留 `treeRss` 字段名，新字段只增不改，`gate.ps1` 不破。

## 3. cap 实验（先修有效性，再谈结论）

- 已知失败模式（`performance-measurement.md:20`）：CDP 端口晚于窗口；启动期导航会关掉 socket，必须重试并重连。cap3 两轮 `cdp timeout` 即此，不能作为证据。
- 有效性门槛（每轮都要过）：
  1. `location.href` 固定为安装版页面；
  2. 连续两次 `tabs.list()` 成功，tab 数与预期一致；
  3. `resumed` 状态在 cap 3/4/6 间一致（要么全 fresh，要么全复用同一份 session 副本）；
  4. 无 `cdp timeout`；出现即作废重跑，并记录原始错误。
- 条件：4 tabs，cap ∈ {3,4,6}，N≥3；同一 exe/cwd/数据目录模板。
- 采集：all-ready、sidecar 数、每 sidecar private、total private/RSS、`driveError`、驱逐事件。
- 切 tab 代价：CDP 激活每个 tab，记录 `tab:set-active` 到 `tab:status ready` 的间隔；各 10 次，median/p95。
- 决策：cap 是两个产品目标的取舍——“能同时 ready 的 tab 数” vs “峰值 private”。有效矩阵出来后再给建议；文档先对齐代码实际值 6，并写明“cap3 行为待对照”。

## 4. 启动归因

- 通道 A：app `boot mark`（`process_start`/`window_visible`）与 renderer mark（`boot:first-paint`/`chat:first-session-rendered`）。
- 通道 B：WPR `CPU + GPU + Handle + VirtualAllocation` 的 `.etl`，按同一 `process_start` epoch 对齐。
- 验收：两通道对同一里程碑差 < 50 ms；否则先修对齐。
- 优化候选（按证据选一个）：WebView2 预热/缓存、bundle 首屏、sidecar 握手（`probe_capabilities` 两次 `--help`）、session replay。

## 5. 生命周期

- 100 次 open/close：`window.omp.tabs.spawn/close` 循环，每 10 次采样 private/RSS/句柄/线程；线性回归斜率 + 95% CI。
- 1 小时 streaming：固定 prompt 间隔，记录 sidecar private 斜率与 transcript 增长。
- 孤儿：正常 `app:quit` 与 `taskkill /PID <app> /T /F` 后，统计 `%LOCALAPPDATA%\Romp` 的 `omp.exe` 与 `msedgewebview2`；目标 0。
- `conhost.exe`：定位产生它的 spawn（对照 `performance-measurement.md:34` 的 `CREATE_NO_WINDOW` 覆盖清单），确认是漏 flags 还是 WebView2/其他子进程。

## 6. Agent runtime 说明

- 事实：`omp.exe` 与仓库内同哈希的三处副本为上游 fork 构建（243.9 MiB，`omp/18.15.0`）；`crates/omp-agent` 是宿主侧进程池/RPC 库，无 `[[bin]]`。
- 文档：`docs/agent-runtime.md` 记录 hash、版本、Private 基线、能力探测、本仓库可控杠杆与不可控项。

## 7. 风险与回滚

- 单实例锁：实验前按名清 `romp-desktop` 并确认无孤儿；实验后恢复正式实例。
- 用户数据：只用隔离 `OMP_DATA_DIR`；resume 用 session 副本，实验后删除。
- 度量噪声：固定电源计划、N≥3、随机顺序；单样本标 preliminary。
- instrument 改动影响 gate：旧字段名保留，新增字段只增不改。
