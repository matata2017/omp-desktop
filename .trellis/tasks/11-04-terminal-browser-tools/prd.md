# PRD：Terminal, browser, tools, PR, memory

父任务：`11-04-full-replication`

## 目标

交付终端、内置浏览器、全部工具卡、PR Center 与 Memory 表面；终端/浏览器归 Rust 所有，不随 agent 释放而丢失。

## 范围（23 行）

`replica/features.csv` 中 `area in (terminal, browser, tools, pr, memory)` 的全部行：

- terminal 4：多 shell、agent pty 接入、生命周期同步、链接/CLI 转发。
- browser 4：面板、CDP/relay、持久登录/休眠、链接路由。
- tools 12：read/write/edit/bash/generic/grep/task/web_search/coordination + 三组工具卡。
- pr 2：PR Center、AI 起草创建。
- memory 1：Memory 窗口。

## 依赖

- core/chat 的事件、transcript、工具卡契约。
- 架构：`omp-terminal`（portable-pty）与 `omp-browser`（WebView2/CDP）先于 sidecar 释放启用。

## 验收

- 终端可开多 shell、输入/resize/关闭、主题跟随；agent pty 自动 reveal。
- 浏览器可开多页、前进/后退/刷新/停止/DevTools、持久登录；链接路由正确。
- 工具卡 registry 全覆盖，未知工具回退 Generic；错误态/折叠正常。
- PR 列表/详情/diff/创建/checkout 可用；Memory 三视图可用。
- `docs/parity.md` 中 23 行全 `done`，每条有证据。
- `cargo test --workspace`、`bunx tsc --noEmit` 全绿。

## 不在范围

系统托盘/菜单/更新/打包；agent 重写。

## 开放问题

无。
