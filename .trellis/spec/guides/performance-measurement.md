# 性能测量与发布版约定

适用于改启动路径、进程生命周期、内存预算或 renderer 文案审计。动手前读一遍，数字以 `docs/perf.md` 为准。

## 发布版必须是 tauri build

`cargo build --release` 的产物仍加载 `devUrl`（`http://localhost:1420`）：不跑 `beforeBuildCommand`、不用 custom-protocol。判据是停掉 Vite 再跑，窗口在但 `window.omp` 不出现、sidecar 不启动、进程树只有约 460 MB 空壳。任何发布版测量前先 `bunx tauri build --no-bundle`。

dev 数字只能做结构相对比较（有/无 sidecar、1/4 tab），不能当预算。

## 单测量器

只用 `scripts/perf/measure.ps1`，输出 JSON 到 `-Out`，退出码 0 成功 / 2 无窗口 / 3 ready 超时 / 4 exe 不存在。配套 `drive.mjs`（CDP 驱动）、`gate.ps1`（预算门）、`profile-stream.mjs`、`probe-shared.mjs`。

几条实测踩过的坑：

- 足迹按 **进程树**（`ParentProcessId` 递归）统计，不按进程名全局匹配，否则把用户自己那套 omp 的 sidecar/WebView2 算进来。
- Tauri 单实例锁按 **app identifier**，不按 exe 路径：debug 实例会顶掉 release 实例。清理按 `romp-desktop.exe` 名字，其他目标按完整路径，别误杀无关 Electron 应用。
- 强杀主进程 **不级联杀子进程**。测量结束用 `taskkill /T`，手动 `-KeepRunning` 验证后要按 `ExecutablePath` 额外清理孤儿 sidecar/stats。
- WebView2 的 CDP 端口晚于窗口句柄出现，且 renderer 装好 `window.omp` 之前不能驱动；启动期导航会关掉 socket，要重试并重连。
- `-Tabs 1` 只测应用自己启动的 active tab。`drive.mjs` 必须**先等应用把 active tab 拉到 ready**，再决定是否激活额外 tab；抢先 `setActive` 会多起一个 sidecar，把 1 tab 测成两个。
- 采样等 `sidecar ready`（按本轮 `epoch_ms` 过滤旧行）后再开始，否则样本横跨进程启动，min/max 差好几倍。

## 启动分段

Rust 侧 `boot` 日志带 `epoch_ms`，renderer 侧 `performance.mark`（`boot:first-paint`、`chat:first-session-rendered`），`drive.mjs` 换算成 wall-clock 对齐。四段：窗口、首帧、sidecar ready、首会话。缺段写 `null`，不要用 0 冒充。

## 进程生命周期

一个 tab 一个 `omp --mode rpc-ui` 进程（约 231 MB）。共享 sidecar 已用 `probe-shared.mjs` 否决：切换会让在途流式回答脱离观察者，事件帧没有会话归属，命令也不带 sessionId。决定见 `docs/architecture.md`。

`omp stats` **懒启动**：`ipc.rs` 的 `stats:url` 和 `stats:fetch` 都 `ensure_running()`，setup 不预启动。只删 setup 的 spawn 会让 `/trace`（先调 `stats.url()`，不挂载统计组件）一直报“未就绪”。省下的 97 MB 是拿 `/trace` 换不得的，改完必须 CDP 实跑一次 `stats.url()`，确认调用前 0 个、调用后 1 个本仓库 `omp.exe stats` 子进程。

## 预算

固定开销约 480 MB + 每 tab 约 240 MB。`gate.ps1` 回归超过 `-MaxRegressionPct`（默认 10%）或 `readyTimedOut` 就 exit 1。冷启 250–655 ms、首帧 1.7–4.6 s 的方差较大，取中位数，单次结果不可当预算。

## renderer 改动

`src/renderer` 是上游复用，字节级改动必须登记 `docs/handoff.md` 的分叉清单。改 locale 要同步 `en/zh/ru` 三个文件、保持占位符、重跑 `vitest run src/renderer/locales`。UI 审计用 `bun run ui:audit`，需要窗口已渲染完；空白页会多报 `landmark-one-main`。axe 违规要按当前可见状态解读，不同页面状态命中不同。
