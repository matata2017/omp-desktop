# PRD：Panels, files, stats

父任务：`11-04-full-replication`

## 目标

交付 Inspector 抽屉、文件/Git 面板与统计仪表盘，并让这些表面在 sidecar 释放后仍可用。

## 范围（16 行）

`replica/features.csv` 中 `area in (panels, files, stats)` 的全部行：

- panels 6：7 视图抽屉、Overview、Queue、上下文指示、MCP、Diff 四 scope。
- files 5：文件树、预览/@mention、打开/显示/编辑器、Git 变更、多根工作区。
- stats 5：仪表盘外壳、Overview、Models、服务托管、其余 8 路由。

## 依赖

- core 的 `omp-session` / IPC。
- 架构：`omp-vfs` / `omp-search` / `omp-git` / `omp-stats` 在 Rust 侧；不依赖 agent 在线。

## 验收

- 抽屉 7 视图可切换，宽度/紧凑态正确；chat tab 只留 overview/btw/files。
- 文件树/预览/图片/路径菜单/搜索正常；Git 状态与 diff 刷新正常。
- Diff 四 scope、unified/split、多文件选择可用。
- Stats 10 路由、范围切换、手动 sync 正常。
- `docs/parity.md` 中 16 行全 `done`，每条有证据。
- `cargo test --workspace`、`bunx tsc --noEmit` 全绿。

## 不在范围

terminal/browser/tools、system/packaging。

## 开放问题

无。
