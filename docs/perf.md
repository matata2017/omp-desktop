# 性能实测（dev 构建）

测量时间：2026-11-04。方法脚本：`scripts/measure-perf.ps1`。

**这些数字来自 dev 构建**（`target/debug/romp-desktop.exe` + Vite dev server，未压缩、带 sourcemap、
HMR 客户端）。它只能用来比较「有 sidecar / 没 sidecar」「1 tab / 4 tab」这类结构差异，不能当成
发布版预算。发布版数字需要 `cargo tauri build` 后重跑同一脚本，尚未做。

## 方法

1. 杀掉旧实例 → `Start-Process` → 轮询 `MainWindowHandle != 0` 记冷启时间。
2. 等到 `runtime.log` 出现 `sidecar ready` 再采样（否则样本会横跨进程启动，min/max 差 7 倍）。
3. 采样 10 次、每次间隔 250 ms，取中位数。
4. 足迹按**进程树**统计（从 `romp-desktop` 的 pid 递归 `ParentProcessId`），再按角色拆开：
   `app` / `webview2`（`msedgewebview2.exe`）/ `sidecar`（`omp.exe --mode rpc-ui`）/ `stats`（`omp.exe stats`）。
   按进程名全局过滤会把用户自己的 omp 实例算进来，已避免。

## 冷启动

| 指标 | 实测 |
|---|---|
| 进程启动 → 窗口句柄出现 | **137 / 141 / 233 / 321 ms**（四次运行） |

计划预算是 < 1500 ms，dev 构建下已有 4–10 倍余量。注意这里只测到「窗口出现」，页面的首条会话
渲染（`sidecar ready` + `get_messages`）另算：`runtime.log` 里 `romp run() start` → `sidecar ready`
通常 2–4 s，其中 `spawn → ready` 与 handoff 记录的 600–950 ms 一致，剩余是大头在首个页面渲染。

## 内存足迹

### 状态①：启动空闲（1 tab，未发任何 prompt）

| 角色 | 常驻 |
|---|---|
| app（主进程） | 66 MB |
| WebView2 子进程（合计） | 530 MB |
| agent sidecar（`omp --mode rpc-ui`） | 246 MB |
| stats dashboard（`omp stats`） | 96 MB |
| **进程树合计（中位数）** | **883–894 MB** |

单次采样区间 348–973 MB：低值出现在采样跨越 GC / WebView2 尚未完成初始化时，这也是为什么脚本
改成「等 `sidecar ready` 再采样」。

### 状态③：4 个 tab（各自一个常驻 sidecar，未发 prompt）

| 角色 | 常驻 |
|---|---|
| app | 90 MB |
| WebView2 子进程（合计） | 477 MB |
| agent sidecar × 4 | 706 MB |
| **进程树合计** | **1273 MB** |

**每多一个 tab ≈ +235 MB**（一个常驻 agent 进程）。这是预算的主变量，不是主进程。

## 与计划预算的差距

计划的目标是 ①启动空闲 ≤120 MB、②1 tab 一次 prompt 后 ≤250 MB、③4 tab ≤500 MB。按上面的实测：

- ①120 MB **不可能**达到：光是 4 个 tab 的 sidecar 就 706 MB，1 个 tab 的 sidecar 也 246 MB，
  而 stats dashboard 再占 96 MB。要进 120 MB 只能不常驻任何 agent 进程。
- ②250 MB 与「一个常驻 sidecar」本身持平，等于要求主进程 + WebView2 只占 4 MB。
- ③500 MB 需要 4 个 tab 共享 1 个进程，与每 tab 一个 sidecar 的隔离设计冲突。

结论：**预算应按「每 tab 250 MB」计**，主进程 + WebView2 是固定开销（dev 下 600 MB，发布版应显著更低，
待测）。这与计划里「每 tab 一个 232 MB 的 sidecar 是主因，故按 tab 计预算」的判断一致，只是数值需要
按 dev 实测上调，发布版另行测量。

## 已经落地的省内存措施

1. **懒启动**：sidecar 只在 tab 首次可见时 spawn（`tab:get-all` / `tab:set-active` / `tab:set-view`）。
2. **布局恢复只启 active tab**：重启时按 `tab-layout.json` 注册全部 tab，但只为 active 那个起进程。
3. **原地会话切换**：同 workspace 点会话用 `switch_session`，不新起 244 MB 进程。
4. **LRU 回收**：超过 `ROMP_MAX_SIDECARS`（默认 3）时回收最久未活动的空闲 tab，可 resume。

## 口径说明

- 足迹按**进程树**统计（从 `romp-desktop` 的 pid 递归 `ParentProcessId`），不是按进程名全局匹配——
  后者会把用户自己那套 omp 的 sidecar / WebView2 一起算进来。
- WebView2 那一行是**多个 helper 进程之和**（browser / gpu / renderer / utility），不是单个进程。
- `WorkingSet64` 是工作集，不是提交量；OS 在内存压力下会 trim，所以同一状态多次采样会出现
  348–973 MB 这种区间。
- 全部数字来自 **dev 构建 + Vite dev server**（未压缩、带 sourcemap、HMR 客户端常驻），只能做
  相对比较；发布版必须用 `cargo tauri build` 的产物重跑本脚本。

## 已知问题

- **正常退出已闭环（实测）**：`window.omp.app.quit()` → 应用退出、`D:\rust-omp` 孤儿进程 **0 个**、
  runtime.log 无 panic（此前 `app:quit` 在 async 命令里 `block_on` 会 panic，已改同步 `terminate_now()`）。
- **强制终止会留下孤儿子进程**：`Stop-Process` 杀掉主进程时，`kill_on_drop` 不生效，sidecar 与
  stats 会存活（实测一次测量跑出 5 个 `omp stats` 残留）。正常关窗 / `app:quit` 现在会先
  `bench.abort()`、关闭全部 tab、`stats.stop()` 再退出（本轮补上）；要覆盖强杀需要 Windows Job Object。
- **stats dashboard 常驻 96 MB**：蓝图要求 app 启动即起；如果预算优先，可改为首次打开统计页再起
  （`stats:fetch` 已经是懒 `ensure_running`，只是 setup 里预启动了一次）。
- dev 构建的 WebView2 内存明显偏高（477–530 MB），发布版需重测后再下结论。
