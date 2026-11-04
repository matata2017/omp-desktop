# PRD：System, scheduler, packaging

父任务：`11-04-full-replication`

## 目标

交付系统集成、定时任务与 Windows 安装包，使 Romp 可以作为常驻桌面应用运行、后台执行任务并正常发布。

## 范围（22 行）

`replica/features.csv` 中 `area in (system, scheduler, packaging)` 的全部行：

- system 13：原生菜单、`omp://` 深链+单实例、窗口几何、退出守卫、系统集成（通知/对话框/剪贴板/openExternal）、命令面板、自动更新、托盘、诊断面、/tools /jobs /force /copy、sidecar 横幅、反馈、benchmark。
- scheduler 5：任务 CRUD、headless 运行、运行历史/详情、Review 队列、通知铃。
- packaging 4：sidecar 解析/构建、图标与平台元数据、安装包目标/extraResources、NSIS/深链声明。

## 依赖

- 所有功能域稳定后收口（托盘/菜单/更新依赖前序表面）。
- `docs/architecture.md`：托盘/更新在 Rust；scheduler headless 不依赖窗口。
- 打包使用 Tauri NSIS；`bundle.resources` 映射到 `resources/omp.exe`（产品 exe 为 `Romp.exe`，不再撞名）。
- macOS/Linux 由 post-1.0 的 `11-04-cross-platform-release` 接手；本任务只保留平台扩展点。

## 验收

- 菜单/托盘/深链/单实例/窗口几何/退出守卫可用；托盘配额环与运行状态正确。
- 定时任务可创建/编辑/运行/审阅；headless 运行不依赖窗口；通知铃未读/已读正确。
- 诊断面（debug/logs/changelog）、/tools /jobs /force /copy、反馈、benchmark 可用。
- 更新状态机（检查/下载/应用）可用；失败不破坏当前安装。
- `bunx tauri build` 产出 NSIS；安装后 `Romp.exe` 与 sidecar 均可启动；`omp://` 注册。
- `docs/parity.md` 中 22 行全 `done`，每条有证据。
- `cargo test --workspace`、`bunx tsc --noEmit` 全绿。

## 不在范围

macOS/Linux 构建/签名（post-1.0）；agent 重写。

## 开放问题

无。
