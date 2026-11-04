# PRD：Models, settings, themes, i18n

父任务：`11-04-full-replication`

## 目标

交付模型/provider 管理、schema 设置、主题与三语界面，使应用可配置、可切换模型并保持与上游一致的设置面。

## 范围（23 行）

`replica/features.csv` 中 `area in (models, settings, themes, i18n)` 的全部行：

- models 7：模型选择器、模型/推理控制、provider 管理、自定义 provider、角色、用量、侧栏配额。
- settings 11：schema 设置、GUI 偏好、审批模式、运行时开关、管理页、Modes/Extensions/Inventory/Compare/Hotkeys/Onboarding。
- themes 3：内置主题、自定义主题目录、agent/插件主题叠加与 pre-paint。
- i18n 2：三语界面、全局语言切换。

## 依赖

- core IPC/事件；chat 的模型切换入口。
- `get_settings_schema` 能力探测（fork sidecar）。

## 验收

- 模型选择器按 provider 分组、搜索、`set_model` 生效；thinking/fast/slow 与标题栏一致。
- provider 登录/登出/自定义 models.yml/角色/用量窗口可用。
- 设置页由 schema 驱动，写入即时生效；审批模式与运行时开关生效。
- 主题（内置+自定义）切换、三语切换、日期/原生菜单文案一致。
- `docs/parity.md` 中 23 行全 `done`，每条有证据。
- `cargo test --workspace`、`bunx tsc --noEmit` 全绿。

## 不在范围

chat 渲染、panels、terminal/browser、system/packaging。

## 开放问题

无。
