# PRD：Core tabs and sessions

父任务：`11-04-full-replication`

## 目标

交付多 tab 与会话基础设施，使后续所有域都能在“tab 是视图、sidecar 按活动会话启停”的模型上工作。

## 范围（22 行）

`replica/features.csv` 中 `area in (tabs, sessions)` 的全部行：

- tabs 7：多标签池、agent/chat 类型、Tab 条、双栏拆分、布局持久化、多窗口、worktree tab。
- sessions 15：索引、侧栏列表、新建/切换、重命名/删除/归档、内容搜索、跨会话搜索、树、branch/fork、导出、信息、Handoff、分享/导入、workspace 切换、模型恢复、resume 选择器。

## 依赖

- P0/P1 完成（工具链、窗口烟测、事件契约）。
- 架构：`docs/architecture.md` 的激活启动/空闲释放模型。

## 验收

- 开/切 tab 不创建额外 sidecar；只有活动路由持有；切回已释放 tab 先出缓存 transcript。
- `tab:get-all` / `tab:set-active` / `tab:set-view` / `tab:get-session-owner` 行为与上游一致。
- 会话索引搜索、跨会话消息搜索、树/branch/fork/handoff/share/import 全部可用。
- `docs/parity.md` 中 tabs+sessions 22 行全 `done`，每条有证据。
- `cargo test --workspace`、`bunx tsc --noEmit` 全绿。

## 不在范围

chat/models/panels/system 等其他域；agent 重写。

## 开放问题

无。
