# Research: `omp.exe` 身份与已测数字（2026-11-08）

## 1. 二进制身份（哈希实证）

| 路径 | 大小 | SHA256 前缀 |
|---|---|---|
| `resources/omp.exe` | 255,728,128 B (243.9 MiB) | `C968104F2A512E43` |
| `target/release/omp.exe` | 255,728,128 B | `C968104F2A512E43` |
| `%LOCALAPPDATA%\Romp\omp.exe` | 255,728,128 B | `C968104F2A512E43` |
| `C:\Program Files\omp\omp.exe`（跑本会话的宿主） | 202,690,560 B | `2A388D8F5DE07870` |

- 三处同哈希 = 打包拷贝，不是 cargo 产物；`--version` 输出 `omp/18.15.0`。
- 本仓库不编译它：`crates/omp-agent` 只有 lib（`argv.rs`/`sidecar.rs`/`pool.rs`/`batcher.rs`），无 `[[bin]]`；唯一 `fn main()` 在 `examples/probe.rs`。
- 来源：`resources/omp.exe` 被 `.gitignore:13` 排除；README:29 与 `docs/handoff.md` §8 写明从上游 v0.9.35 portable 的 `$PLUGINSDIR/app-64.7z` 解出。
- 宿主 `omp.exe` 是另一个构建（不同哈希/大小），不在 `romp-desktop` 进程树内，不参与 GUI 测量。

## 2. 实测（安装版 22:00:06，隔离 `OMP_DATA_DIR`，Private = `PrivateMemorySize64`）

- 单 sidecar 基线：private 中位 **501.0 MB**（N=3：505.5 / 501.0 / 489.2），RSS ~240 MB。
- 恢复 6.92 MB 真实 session（日志确认 `resume=Some`）：private 中位 **537.2 MB**（541.8 / 537.2 / 534.2），**+36.2 MB**（约 transcript 的 5.2 倍），两组区间不重叠。
- 4 tabs、cap 6（代码默认）：4 个 sidecar 全部 ready，每个 private 508.5 / 523.9 MB；sidecar 合计约 **2.0 GB** private。
- 4 tabs、cap 3（旧文档默认）：只有 2 个 sidecar，`readyTimedOut=true`，drive 报 `cdp timeout` —— 4 tab 跑不满。
- 同刻两种口径：RSS 合计 801.5 MB vs Private 合计 944.1 MB（1 个 sidecar、4 个恢复 tab、1 个启动）。

## 3. 量具坑（均已实测）

1. `treeRss` 是 WorkingSet 求和：共享 DLL 页会重复计入，且与 Private 差 143 MB；报告必须并列两口径。
2. `measure.ps1` 默认在 `finally` 里 `taskkill /T /F`；不加 `-KeepRunning` 时无法二次采样（首次 cap 探针因此全空）。
3. PowerShell 5.1 `Set-Content -Encoding utf8` 写 BOM → `tab-layout.json` 解析失败 → 静默回落 fresh tab（resume 探针曾因此测了两次空会话）。用 `[System.IO.File]::WriteAllText(..., UTF8Encoding($false))`，并以日志 `resume=Some` 为通过条件。
4. `measure.ps1:80` 在 `Tabs>3` 时注入 `ROMP_MAX_SIDECARS=$Tabs`；cap 实验必须显式设置并记录。

## 4. 文档/代码漂移

- 旧文档：README:25/60、`docs/architecture.md`:146、`docs/parity.md`:210 写“默认 3”。
- 代码：`src-tauri/src/ipc.rs:1481` 为 `unwrap_or(6)`；`ab4cf98` 引入 `max_sidecars()` 时就是 6；无测试钉住该值。
- 结论：6 是实际行为，且 3 无法让 4 tab 全部 ready；已把三处文档改为 6，并补 private 预算口径。
