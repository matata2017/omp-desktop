# PRD：Chat and agents surface

父任务：`11-04-full-replication`

## 目标

交付核心对话回路与代理表面：发 prompt → 流式 markdown → 工具卡 → agent_end，以及 steer/follow-up/abort、审批、子代理观测。

## 范围（27 行）

`replica/features.csv` 中 `area in (chat, agents)` 的全部行：

- chat 20：流式渲染、steer/follow-up、abort、markdown、代码块、工具卡、虚拟化 transcript、扩展 UI/审批、思考块、KaTeX/Mermaid、图片、@file/slash、plan 审批、查找、dock、用量、语音、BTW、composer 编辑器、ask。
- agents 7：Agent Hub、侧栏 agent、Collab、roster、子代理页、hover 卡。

## 依赖

- core tabs/sessions 完成（transcript 缓存、agent 进程管理）。
- P1 事件契约与 32ms batcher。

## 验收

- 一条 "list files in crates" 回合：流式文本、至少一个工具卡、agent_end 正确。
- 流中 steer、⌘Enter follow-up、Esc abort 均可用；队列状态与上游一致。
- 审批/扩展 UI/ask 的 payload 与超时行为正确（ask 提交 `kind=submit` + `results[]`）。
- 子代理 roster/page/hover/BTW 可打开并实时更新。
- `docs/parity.md` 中 chat+agents 27 行全 `done`，每条有证据。
- `cargo test --workspace`、`bunx tsc --noEmit` 全绿。

## 不在范围

模型选择/设置/面板/终端/浏览器等其他域。

## 开放问题

无。
