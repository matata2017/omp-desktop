# 流程清册（F01–F26）

> 源：本仓库 `src/renderer`（上游 tag v0.9.35，MIT）+ `D:/tmp/ompgui-src/src/main`（0.9.35 工作副本）。下文路径均相对 `src/`。
> 本次实读范围：`src/renderer/App.tsx`、`hooks/`、`stores/`、`lib/`、`components/{layout,chat,dialogs,panels,settings,tasks,stats,tools}/`、`src/shared/{ipc-types,rpc-types,rpc-client,approval-prompt,scheduler-types}.ts`、`src/main/{ipc,updater,deep-link,browser-host}.ts`、`src/main/scheduler/*`。
> `文件:行` 均为本次实读锚点；未读到的实现只写符号名，不猜行号。
> 屏幕 ID 由 screens 分册定义，本册用「表面名」指代（TitleBar / Sidebar / ChatStream / InputArea / PanelContainer / 各 Dialog）。步数 = happy path 上用户可见或需一次 await 的步骤数。

## 总表

| ID | flow | 步数 | screens/steps | edge cases | evidence |
|---|---|---|---|---|---|
| F01 | 发送 prompt 并观察流式回复 | 7 | InputArea composer → 乐观用户气泡 → ChatStream/`StreamingRows`（`StreamingText` + `ThinkingBlock live`）→ `TurnStatusRow` → `agent_end` 结算 | 空文本/仅图片；`sending`/`submissionUncertain`；`rpc_delivery_unknown`；`prompt_result` 拒收；slash 分流；busy 时 steer vs followUp；后台 tab 只收状态；关 tab 丢帧；`switchPending` 丢帧 | `renderer/components/layout/use-composer-submit.ts` `send`；`renderer/lib/composer-submit.ts` `planComposerSubmit`；`renderer/stores/messages.ts:218` `applyEvents`；`renderer/hooks/use-rpc-events.ts:330,377`；`renderer/components/chat/ChatStream.tsx:106-209,966-1115`；`renderer/components/chat/StreamingText.tsx:36` |
| F02 | 工具卡生命周期 pending→running→done/error/aborted | 6 | assistant `toolCall` 块 → `useToolsStore` 条目 → `ToolCard` 状态条/时长 → 展开体（各 tool renderer） | 同 id 多次调用；参数流式（`streamingArgs`）；缺 `tool_execution_end`；`isError` 与 status 并存；`todo` 结果旁路；历史重建把未完成调用标 `aborted` | `renderer/stores/tools.ts:14,31,59,188,249,277,299,309,318`；`renderer/components/tools/ToolCard.tsx:40-99` |
| F03 | 中断/中止一次运行 | 4 | 停止按钮（`Square`）/ Esc → `abortActiveTurn` → `abort_retry`+`abort` RPC → `agent_end(aborted)` 静默收尾 | 重试等待窗口；overlay/dialog 吃掉 Esc；IME 组合；bash/eval/终端各自的取消路径；`defaultPrevented` | `renderer/components/layout/InputArea.tsx:1196-1206`；`renderer/App.tsx:564-575`；`renderer/lib/messages.ts:80-95`；`shared/rpc-client.ts:102-113` |
| F04 | 切换模型 / 思考等级 | 5 | ⌥M/标题栏 → `ModelPicker`（分组/搜索/登录态）→ `set_model` → `model_changed`；composer `ThinkingControl` 菜单 → `set_thinking_level` | 模型无 thinking ladder；`auto` 与有效等级双字段；`overContext` 触发压缩后 hydrate；provider 未登录；切换失败仅 toast 不改 store | `renderer/components/dialogs/ModelPicker.tsx:191-221`；`renderer/components/layout/ThinkingControl.tsx:115-140`；`renderer/hooks/use-rpc-events.ts:510`；`renderer/stores/model.ts:16-26,163`；`renderer/App.tsx:475` |
| F05 | 新建 tab / 切换 / 关闭 / 分屏 | 5（新开 3 / 切换 1 / 关闭 2 / 分屏 3） | TabBar `+`/⌘T → `openTab` → `tabs:spawn` → `switchTab`；chip 点击；×/⌘W；右键 4 向 placement 或拖拽到边缘 | `kind-mismatch`；F-OWN 双挂载拒绝；spawn 首帧先到；最后一 tab 不可关；流式中关闭需内联确认；分屏最多 2 可见 | `renderer/stores/tabs.ts:56-109,349,542-643,645-695,718-760`；`renderer/components/layout/TabBar.tsx:325-330,786-801,869-893`；`renderer/components/layout/SplitWorkspace.tsx:31-36`；`renderer/lib/tab-routing.ts`；`main/ipc.ts:721-763` |
| F06 | 会话列表浏览/搜索/打开/重命名/删除 | 6 | Sidebar 全局列表 → 搜索框/全局搜索 → 行点击打开 → 行内重命名 → 删除确认 → `sessions:changed` 刷新 | 空列表 vs 读取失败；scheduled run 过滤；`.jsonl` 校验；运行中拒删；活动会话走 `drop_session`；hook 取消删除 | `renderer/components/layout/Sidebar.tsx:298,588-690,1201-1208`；`renderer/hooks/use-session-list.ts:29-99`；`renderer/hooks/use-session-switch.ts:267-291`；`main/ipc.ts:622-679,822-841` |
| F07 | 恢复历史会话（⌘P） | 5 | ⌘P → `SessionPickerDialog`（scope/sort）→ 本地排序 + 内容搜索 → 选中 → `switchSessionNow`/`requestSessionSwitch` → hydrate | 已属于别的 tab；kind 不匹配；`session_owned_elsewhere`；模型缺失；hook veto；忙时切新 tab/窗口；同 id 直接返回 | `renderer/components/dialogs/SessionPickerDialog.tsx:128-180`；`renderer/hooks/use-session-switch.ts:150-235,284-291`；`renderer/lib/session-search.ts`；`main/ipc.ts:816-818` |
| F08 | 从消息 branch / fork | 5 | 消息悬浮操作 → `fork_from`（新 tab）或 `branch`（回填 composer）；SessionTree 图节点 → `switch_leaf`/`branch`/`fork_from`（新窗口）；BranchPicker 列表 → branch | 非 user 节点不可 branch；旧消息无 `entryId` 需树解析；`cancelled`/`reopenAsk`；源 tab 已切；新窗口失败；摘要切换长任务 | `renderer/lib/messages.ts:98-208`；`renderer/components/chat/MessageBubble.tsx:447-463`；`renderer/components/dialogs/SessionTreeDialog.tsx:91-108,440-548`；`renderer/components/dialogs/BranchPickerDialog.tsx:41-63` |
| F09 | 压缩（compact）与 handoff | 4 / 6 | `/compact` 或命令 → `rpc.compact()`；自动压缩事件驱动状态行；handoff dialog → `handoff` → 校验 session → `fork` → hydrate | compact 被拒（太小/已压缩）不恢复草稿；`rpc_delivery_unknown` 恢复；自动压缩 aborted；handoff 运行中不可关；stop 后不 fork；sessionId 变化放弃 | `renderer/lib/composer-submit.ts:36,121-124`；`renderer/components/layout/use-composer-submit.ts` compact 分支；`renderer/hooks/use-rpc-events.ts:390-410`；`renderer/components/dialogs/HandoffDialog.tsx:58-137`；`renderer/stores/fork-handoff.ts:22` |
| F10 | plan mode 提案与审批 | 7 | 开 plan mode（ComposerModes/⌥⇧P/PlanDockCard/palette）→ agent 写 `xd://propose` → `plan_proposal` → `PlanApprovalDialog` → `plan_approval` → 结算/刷新 | 未 ready 禁用；latest-wins；save 需先选路径；accept 后服务端静默退出 plan mode；approve-and-compact 可 Stop；关 tab 后提案丢弃 | `renderer/stores/plan-approval.ts:12-55`；`renderer/hooks/use-plan-approval.ts:17-42`；`renderer/components/dialogs/PlanApprovalDialog.tsx:95-181`；`renderer/components/layout/ComposerModes.tsx:133`；`renderer/App.tsx:475-484` |
| F11 | 权限确认（tool approval）审批 | 6 | agent `select` 请求 → `extension_ui_request` → 按 tab 入队 → 最旧一条 → `ApprovalDialog`（read/write/exec badge + grant 行）→ `extension_ui_response` → 清队列 | 超时倒计时自动 cancel；`BLOCKING_UI_METHODS` 驱动标题/信号；非对话框方法；`open_url` 内嵌 authCode；后台 tab 请求先存 | `renderer/components/dialogs/ExtensionDialog.tsx:523,644-720`；`renderer/components/dialogs/ApprovalDialog.tsx:65-150`；`renderer/hooks/use-extension-ui.ts:17-38`；`shared/approval-prompt.ts:9-35`；`shared/rpc-types.ts:1993`；`main/ipc.ts:591-598` |
| F12 | 子代理 spawn 与 DAG 视图 | 6 | `task` 工具 → subagent 帧/`get_subagents` → store 合并 → Agents dock/面板 `agents` → Agent Hub `hub` 表格 → DAG 节点 → transcript | 服务端遗忘 terminal 行；live 行缺失即丢；advisor 只读；main_agent 不可 abort；DAG 未解析父边；仅在 streaming/有 agent 时轮询 | `renderer/stores/subagents.ts:12,42,75-130`；`renderer/hooks/use-subagent-stream-poll.ts:18-35`；`renderer/components/panels/SubagentDag.tsx:136-240`；`renderer/components/panels/AgentHubWindow.tsx:60,984-1022`；`renderer/stores/subagent-graph.ts` |
| F13 | todo / queue dock | 5 | `todo` 工具结果 → `setPhases` → `TodoDockCard`（勾选/拖拽 → `set_todos`）；`queue_update` → `QueueDockChip` → `QueuePanel`（queue_move/remove/clear） | 空即隐藏；写入失败回滚；lane 顺序服务端 clamp；扩展命令不能走 follow_up；空 queue 简写只提示；dequeue 回填 | `renderer/stores/todo.ts:117-140,161-171`；`renderer/components/chat/dock/TodoDockCard.tsx:300-372`；`renderer/stores/queue.ts:16-60`；`renderer/components/panels/QueuePanel.tsx:1-9,44-60`；`renderer/components/chat/dock/QueueDockChip.tsx:16-69` |
| F14 | 文件面板浏览 + diff 视图 | 6 | 右侧 panel `files` → `fs:list` 树 → 点击 `fs:read` 预览 → `@` 插入；`diff` tab → 从 tools store 取 edit 结果 → 统一/分栏 + Timeline | 2000 项/深度 8 截断；二进制/超大；无工作区；读取竞态版本号；无编辑结果空态；chat tab 仅 files | `renderer/components/panels/FilesPanel.tsx:98-176`；`main/ipc.ts:1151-1318`；`renderer/components/panels/DiffPanel.tsx:1-62`；`renderer/components/layout/PanelContainer.tsx:34-42,74,213-220`；`renderer/stores/ui.ts:337-345` |
| F15 | 内置浏览器 open/navigate/capture | 6 | panel `browser` → `browser:get-state` → `browser:open` → ResizeObserver `set-viewport` → `browser:action` → 遮挡时 `browser:capture` still | 只有同窗口一个 tab 显示视口；页面加载失败 `loadError`；popup 变面板页；keys 交回应用；stock sidecar 无 relay 时降级外部浏览器 | `renderer/components/panels/BrowserPanel.tsx:173-274,432-468`；`renderer/stores/browser.ts:8-43`；`main/ipc.ts:129-141,773-806`；`main/browser-host.ts:27-30,75-76,858-870,978-1049`；`shared/ipc-types.ts:974-1000` |
| F16 | 终端打开与运行 | 5 | panel `terminal` → `terminal_list` → `terminal_open` → xterm attach + `terminal_snapshot` → input/resize 帧 → 关闭 | 自动开首个终端；`exitCode` 保留；关 tab 清理视图；主题切换重设主题；输出 seq 缺口重 snapshot；agent `pty` 会话可见 | `renderer/stores/terminals.ts:32,145-162,200-298`；`renderer/components/panels/TerminalPanel.tsx:128-206`；`shared/rpc-types.ts:740-816` |
| F17 | 定时任务创建/运行/复核 | 7 | 打开 Tasks 窗口 → 4 视图 → New 表单（200ms 去抖 validate）→ create/update → run-now 或 cron 触发 → progress 帧 → Review accept/reject | 校验问题项；重叠/积压跳过；`blocked_config`；取消 queued/运行；app 退出取消；timeout；失败连击自动 disable；worktree 清理 | `renderer/stores/scheduler.ts:21-53,92-115,122-207`；`renderer/components/tasks/TasksWindow.tsx:64-118`；`renderer/components/tasks/TaskForm.tsx:28-40,501`；`renderer/components/tasks/RunsView.tsx:136-226`；`main/scheduler/scheduler.ts:222,326-394`；`shared/scheduler-types.ts:18,72-104,160` |
| F18 | 统计看板 | 5 | `open-stats` → `StatsDashboard` → 打开自动 `POST /api/sync` → `stats:fetch` 各路由 → range 切换重拉 | stats 服务未起 → unavailable + 静默重试；非白名单路径报错；列表接口形状校验；`stats:url` 触发 revive | `renderer/components/stats/StatsDashboard.tsx:24-38,42-70,167-176`；`renderer/hooks/use-stats.ts:44-117`；`renderer/lib/stats-sync.ts`；`main/ipc.ts:844-878` |
| F19 | 设置 schema 编辑 | 6 | ⌘, → `SettingsWindow` 并行 `get_settings_schema`+`get_settings` → schema tab → 改行 → `set_setting` → 回读 effective value / provenance | 过期回包丢弃；`restoreUnset`；JSON 解析失败；schema 失败仍可看 GUI 页；非 ready 禁用；GUI 偏好另走 `prefs:set` | `renderer/components/settings/SettingsWindow.tsx:1-22,286-300`；`renderer/components/settings/SchemaSettingRow.tsx:164-224`；`renderer/components/settings/pages/SchemaTabContent.tsx:26-49`；`renderer/stores/settings.ts:7-52,224`；`main/ipc.ts:1022-1063` |
| F20 | 主题切换 | 5 | 打开 theme picker → 列表（内置/system/自定义）→ 搜索/选择 → `applyThemeByName` + 持久化 → `setTheme` 同步旧 store | 自定义主题坏文件；`followAgentTheme` 跟随 agent；插件主题会话级清理；系统主题变化仅在 system 模式重应用 | `renderer/components/dialogs/ThemePickerDialog.tsx:1-9,143-147`；`renderer/lib/themes.ts:1696,1723,1791,1851,2221`；`renderer/App.tsx` boot prefs 效果；`renderer/public/pre-paint.js`；`renderer/stores/ui.ts:403,557` |
| F21 | 命令面板 / slash 命令 | 6 | ⌘K → 最近项 + `get_available_commands` → 模糊过滤/子菜单 → Enter `execute` → affordance 分派 → 错误 toast；composer 内 `/` 走服务端 prompt 或本地 affordance | 未 ready 仅白名单可用；`unavailable` 提示；命令拉取失败可重试；`/compact` `/clear` `/new` `/share` 等特判；GUI-only 内置不落成文本 | `renderer/components/dialogs/CommandPalette.tsx:43,107-146,238-255,503-521`；`renderer/lib/command-registry.ts:68,260,1334,2001`；`renderer/lib/composer-submit.ts:23-137`；`renderer/components/layout/InputArea.tsx:276-300` |
| F22 | @file 提及 | 4 | composer 输入 `@` → provider 链 → 文件模糊（≤20）+ 内部 scheme → 选中替换/插入 `@path`（含空格转 `@"…"`） | 列表读取失败仅 scheme；光标移动则不替换；chat tab 无模型 mention；仅行首/空白后触发 | `renderer/components/layout/use-completion-menu.ts:34,87-90,226-243`；`renderer/components/layout/input-area-utils.ts:13-16,54-75,104-109`；`renderer/components/panels/FilesPanel.tsx:174-176`；`renderer/components/layout/InputArea.tsx:351` |
| F23 | 语音听写 | 6 | 点麦 → `dictation_start` + 开麦并行 → `host_dictation_*` 帧 → 实时落字 → transcribing → 最终提交（可选 submit 触发发送） | 麦克风权限拒绝；start 返回 idle；切 tab/卸载取消；下载模型时关麦；transcribing 再点只结束；行首/空白间距 | `renderer/lib/dictation.ts:13-22,74-186,288-317,328-348`；`renderer/components/layout/use-dictation.ts:38-63`；`renderer/components/layout/InputArea.tsx:534,944-945`；`renderer/components/layout/DictationStrip.tsx` |
| F24 | 更新流程 | 6 | 启动 3s/每 4h 自动检查或手动 → `checking` → `available` banner → `updater:download` → `downloading` → `downloaded` → `updater:apply` | macOS manual 安装包选架构；缺安装包/不支持架构；被动失败静默；检查无结果转 not-available；按版本/错误持久化 dismiss | `main/updater.ts:47-61,200-330`；`renderer/stores/updater.ts:20-91`；`renderer/components/layout/UpdateBanner.tsx:19-117`；`shared/ipc-types.ts:330-344,128`；`renderer/App.tsx:634` |
| F25 | 深链 `omp://` | 4 | OS 拉起 → 协议注册/第二实例 argv → 解析 new/session → IPC `deep-link` → App 处理（新会话或切会话） | 首个实例 handler 未注册 → 队列；非法 host 忽略；会话不存在 toast；流式中走切换对话框；跨 kind 开新 tab；新窗口 pending 一次性消费 | `main/deep-link.ts:21-23,44-60,97-110`；`renderer/App.tsx:380-410`；`shared/ipc-types.ts` `DEEP_LINK`；`main/ipc.ts:682-703`；`renderer/hooks/use-rpc-events.ts` ready 分支 |
| F26 | 自动重试与模型回退 | 5 | 失败 → `auto_retry_start` → `TurnStatusRow` 倒计时 → `auto_retry_end` → 成功续流/失败 toast；`retry_fallback_*` 提示回退 | 重试窗口内静默通知；`agent_end` 可能中途到达（isTerminal false）；重试成功可能无 `turn_start`；用户中断先 `abort_retry`；屏外才 toast | `renderer/hooks/use-rpc-events.ts:412-470,60-90`；`renderer/components/chat/ChatStream.tsx:1146-1200`；`renderer/stores/session.ts:47-59`；`shared/rpc-types.ts` `auto_retry_*` |

## 逐流说明

### F01 发送 prompt 并观察流式回复（7 步）

**happy path**
1. composer 输入后按 Enter / 点发送：`InputArea.tsx:515` 的 `useComposerSubmit({...})` 返回 `send`，发送按钮 `InputArea.tsx:1170-1195` 传入 `"steer"`/`"followUp"`。
2. `use-composer-submit.ts:send` 守卫：空文本且无图片、`sending`、`!routeReady || !runtimeTabId` 直接返回；`status !== "ready"` toast「agent 连接中」；`collab.readOnly` toast（约 71-95 行）。
3. 文本展开：`expandEmoticons(expandPasteMarkers(message))`（同文件 ~120-128）。
4. `planComposerSubmit`（`lib/composer-submit.ts:99`）选择通道：idle → `rpc.prompt`；streaming → `rpc.steer` / `rpc.follow_up`；返回 `{kind:"send", request}`。
5. 非流式非 slash：追加 `optimistic` 用户消息 `appendLiveMessage`，`pinTranscriptToBottom()`，清 draft/images，`setSending(true)`，`setTimeout(0)` 后发请求（~400-440 行）。这样 Enter 到 bridge 序列化之间不会看起来「没反应」。
6. 成功：`useInputHistoryStore.record`、`settleComposerResponse`（仅本地执行类命令才 hydrate）。失败：移除乐观消息、`restoreDraft`、toast；`response.code === "rpc_delivery_unknown"` → `markUncertain()` + `submissionUncertain`。
7. 事件与渲染：`agent_start` → `isStreaming=true`、`awaitingModelSince=now`、`setBrowsing(tab,false)`、`refreshSessionState`（`use-rpc-events.ts:330-355`）；`message_start` 建 `streamingMessage` 并清缓冲，`message_update` 的 `text_delta`/`thinking_delta` 累积（`stores/messages.ts:218-250`）；`message_end` 收进 `liveMessages`；`agent_end` upsert 已提交消息、`isStreaming=false`、按通知策略推送（`use-rpc-events.ts:377-388`）。视图由 `ChatStream.tsx` 的 `StreamingRows` 渲染 `ThinkingBlock live` + 运行中工具卡 + `StreamingText`（1087-1113），`TurnStatusRow` 承担 retry/compact/awaiting-model 状态行（205-209, 1146）。

**边界**
- 服务端收下后拒绝（缺凭据 / Provider 被删）：`prompt_result` 帧 → `returnUnsentPrompt` 把乐观气泡撤回 composer（`prepend:true`）并 toast（`use-rpc-events.ts:195-225`）。
- 投递结果未知：不清草稿、不重发，标 `submissionUncertain` 等 hydrate 对账（`use-composer-submit.ts` `markUncertain`）。
- 后台 tab：全量事件只推 focused + 最多 1 个可见分屏 tab；后台只推 `tab:status`，settle 时由 `notifyBackgroundSettle` 出 toast/系统通知。
- 已关闭 tab：`closedTabFrame` 丢弃迟到帧；`switchPending` 期间丢弃目标会话之前的帧（`use-rpc-events.ts:reduceEvents` 首段）。
- chat tab：无工具，右侧面板只剩 files（`PanelContainer.tsx:74`）。

**证据**
- `renderer/components/layout/InputArea.tsx:515,1170-1206` — `useComposerSubmit`、发送/停止按钮。
- `renderer/components/layout/use-composer-submit.ts` — `send`、乐观气泡、`markUncertain`、bash/python/queue 分支。
- `renderer/lib/composer-submit.ts:99-137` — `planComposerSubmit`、`settleComposerResponse`。
- `renderer/stores/messages.ts:215-390` — `createMessagesStore.applyEvents`、`appendLiveMessage`、`reconcileFetched`。
- `renderer/hooks/use-rpc-events.ts:300-390` — `useRpcEvents`/`reduceEvents`、`notifyOnAgentEnd`、`toastOnAgentError`、`returnUnsentPrompt`。
- `renderer/components/chat/ChatStream.tsx:92-115,181-260,966-1115,1146-1250`；`StreamingText.tsx:36`；`ThinkingBlock.tsx:66`。

### F02 工具卡生命周期（6 步）

**happy path**
1. assistant 消息里的 `toolCall` 块（参数流式期间用 `toolcall_delta`）→ `tools` store 生成 `status:"pending"` 条目，`streamingArgs` 保存原始 JSON 片段（`stores/tools.ts:249,277`）。
2. `tool_execution_start` → `status:"running"`、`startTime=Date.now()`（299-315）。
3. `tool_execution_update` → `partialResult`（309-317）。
4. `tool_execution_end` → `status: isError ? "error" : "done"`、`result`、`endTime`，并从 running 映射移除（318-332）。
5. `ToolCard` 读 `activeTools.get(toolCallId)`；`pending` 显示为 spinner（视觉按 running），`aborted` 用 warning 色，`isError` 用 error 色，2px 状态轨 + `data-tool-status`（`ToolCard.tsx:51-99`）；展开体按 `getToolRenderer(toolName)` 选渲染器。
6. 结算：`agent_end` 后 `ChatStream` 用 `settledTools` 计算本轮变更摘要（`ChatStream.tsx:181-227`）；历史重建把无结果调用标 `aborted`，仅当前流式 turn 的尾部未完成调用保留 `running`（`tools.ts:26-31,205-209`）。

**边界**
- 同一 `toolCallId` 跨 turn 复用：`toolEntryKey` 按 occurrence 分配，`takeExecutionKey` 复用未完成条目而不是新开卡（`tools.ts:59-61,114-120`）。
- 参数尚未到齐：`args` 仍 `{}`，卡片摘要显示 `streamingArgs.slice(0,160)`（`ToolCard.tsx:62-65`）。
- 进程死亡/中断导致 `tool_execution_end` 缺失：hydrate 重建不出永久 spinner（文件头注释 + 188 行）。
- `todo` 工具的 `tool_execution_end` 额外驱动 todo store（`use-rpc-events.ts` `case "tool_execution_end"` → `todoPhasesFromToolResult`）。

**证据**
- `renderer/stores/tools.ts:5-34,59-120,180-340`。
- `renderer/components/tools/ToolCard.tsx:40-170`；各 `components/tools/*Renderer.tsx`。
- `renderer/hooks/use-rpc-events.ts` `tool_execution_end` 分支。

### F03 中断/中止一次运行（4 步）

**happy path**
1. 流式中发送按钮换成停止按钮（`Square`）→ `abortActiveTurn(rpc, runtimeTabId)`（`InputArea.tsx:1196-1206`）；或 Esc（`App.tsx:564-575`，要求无 overlay、无 `role=dialog`、`!event.defaultPrevented`）。
2. `abortActiveTurn`：若 `retryInfo` 存在，先 `resetRetryPending`、清 retry UI、`rpc.abortRetry()`（失败吞掉），最后统一 `rpc.abort()`（`lib/messages.ts:80-95`）。
3. RPC：`{type:"abort"}`（`shared/rpc-client.ts:102`）；另有 `abort_bash`、`abort_eval`、`abort_retry`、`abort_subagent`（102-113,124）。
4. 服务端收尾：`agent_end` 的最后一条 assistant `stopReason === "aborted"` → 不发完成/错误通知（`use-rpc-events.ts:notifyOnAgentEnd`），`isStreaming=false`。

**边界**
- Esc 只在没有 overlay/dialog 且事件未被消费时生效；IME 组合中 `isImeKeyEvent` 直接返回。
- 重试窗口：先 `abort_retry` 再 `abort`，避免「停了又重试」。
- `$ code` 的运行气泡：cancel → `abortEval`（`rpc-client.ts:112`）；`!cmd` 的 bash 气泡走独立响应，不由 `abort` 处理。
- 终端：关闭终端才杀程序（`terminals.closeTerminal` → `terminal_close`），`abort` 不作用于 PTY。
- 子代理：`abort_subagent(agentId)` 是独立路径；主 agent 不可 abort（Agent Hub 拒绝理由 `main_agent`）。

**证据**
- `renderer/components/layout/InputArea.tsx:1196-1206`。
- `renderer/App.tsx:564-575`。
- `renderer/lib/messages.ts:80-95`（`abortActiveTurn`）。
- `shared/rpc-client.ts:102-124`。
- `renderer/hooks/use-rpc-events.ts` `notifyOnAgentEnd`（`stopReason === "aborted"` 早退）。

### F04 切换模型 / 思考等级（5 步）

**happy path（模型）**
1. ⌥M 或标题栏/命令 → `ui.openModelPicker()`（`App.tsx` keymap `model.select`；`stores/ui.ts:360`）。
2. `ModelPicker` 打开时拉 `getAvailableModels`，按 provider 分组、搜索、标记登录状态与当前模型（`ModelPicker.tsx:56-190`）。
3. 选中 → `tabRpc.setModel(provider, modelId)`（209）。
4. 成功 → `applyModelInfo(response.data, tabId)`；若本次切换前处于 over-context（压缩重写 transcript）→ `hydrateSession()`（217-221）；失败仅 toast，不改 store（210-214）。
5. 服务端 `model_changed` → `refreshModelState(tabId)`（`use-rpc-events.ts:510-512`）；`get_state` 刷新标题栏/上下文环。

**happy path（思考等级）**
1. composer 的 `ThinkingControl` 渲染 `off / auto / 模型 ladder`（`ThinkingControl.tsx:33-53`）。
2. 选择 → `rpc.setThinkingLevel(level)`（115-140）。
3. 成功本地对齐 `thinkingLevel`/`thinkingConfigured`；失败 toast。
4. `thinking_level_changed` 事件写回两个字段（`use-rpc-events.ts` 该分支；`stores/model.ts:16-22`）。
5. ⇧Tab（焦点在 textarea）与 ⌃P / ⇧⌃P 走 `cycleThinkingLevel` / `cycleModel`（`App.tsx:585-600,462-472`）。

**边界**
- 模型不支持 thinking → `availableThinkingLevels` 为空，不渲染菜单。
- `thinkingConfigured`（选择器显示，可为 `auto`）与 `thinkingLevel`（服务端有效值）是两件事；显式变更的事件省略 `configured`，代码按「等于有效值」处理，避免菜单勾选陈旧。
- Provider 未登录仍可列出但需先登录（ModelPicker 的 login 状态）。
- 快速连点：`requestVersion` 丢弃过期目录响应（`ModelPicker.tsx:59`）。

**证据**
- `renderer/components/dialogs/ModelPicker.tsx:55-221`。
- `renderer/components/layout/ThinkingControl.tsx:33-190`。
- `renderer/stores/model.ts:16-26,83-175`。
- `renderer/hooks/use-rpc-events.ts:510-512` 与 `thinking_level_changed` 分支。
- `renderer/App.tsx:462-472,585-600`；`renderer/lib/keymap.ts` 动作表。

### F05 新建 tab / 切换 / 关闭 / 分屏（5 步，分项 3/1/2/3）

**happy path**
1. 新建：`+` 或 ⌘T（agent）/⇧⌘T（chat）→ `openTab({kind})`（`TabBar.tsx:869-893`；`App.tsx:494`）。
2. `openTab` → `window.omp.tabs.spawn({cwd,sessionPath,kind,worktree,defaultWorkspace})`；失败 toast；`refusal==="kind-mismatch"` 报错；`tabId===null` 且带 owner → 切到本窗口 owner tab 或聚焦 owner window；成功 → `ensureTabRuntime` + 乐观 upsert（`status:"starting"`）+ `switchTab`（`stores/tabs.ts:542-637`）。
3. 切换：`switchTab` → `showView(id, groups)`（639-643, 366+）：`beginTabRoute` → `SET_TAB_VIEW`/`SET_ACTIVE_TAB` → 结算 `settleTabRoute`，期间 `acceptsActiveTabEvents()` 为假，旧路由事件不会写进新 tab（`lib/tab-routing.ts`）。
4. 关闭：chip ×/⌘W → `closeTab(id)`；`tabs.length <= 1` 直接返回；先激活邻居再释放；活动 tab 流式中先进入 3s 内联确认（`TabBar.tsx:78,490-493`）；主进程 `CLOSE_TAB` 释放 sidecar（`main/ipc.ts:738-745`）。
5. 分屏：chip 右键 → 左/右/上/下 → `splitTab(id, placement)`；或把 chip 拖到边缘（`placementFromPointer`，`SplitWorkspace.tsx:31-36`）；`SplitLayout{axis,firstTabId,secondTabId,ratio}`，`setSplitRatio` 去抖落盘；`unsplit` 拆对（`tabs.ts:349-355,645-695`）。

**边界**
- spawn 的返回可能晚于该 tab 的第一个 `TAB_STATUS`：upsert 是幂等的（`openTab` set 分支）。
- 后台 tab 结束 → `unreadDone`；失败/重试 → `attention:"failed"|"retrying"`（`SessionTab`，`tabs.ts:56-91`）。
- 分屏不增加「可见」上限：`visibleTabIds` 最多返回两个 id（`tabs.ts:187-190`）。
- 关闭 worktree tab 触发清理对话框（`WorktreeCloseDialog`，`App.tsx` 挂载）。
- 关闭活动 tab 切到邻居；关闭非活动 tab 不动焦点。

**证据**
- `renderer/stores/tabs.ts:56-109,187-190,315-365,542-760,875-905,997+`。
- `renderer/components/layout/TabBar.tsx:136-147,325-330,514-525,556-580,786-801,869-893`。
- `renderer/components/layout/SplitWorkspace.tsx:31-53`。
- `renderer/lib/tab-routing.ts`（`beginTabRoute`/`settleTabRoute`/`reconcileTabRoute`/`acceptsActiveTabEvents`）。
- `main/ipc.ts:721-763,808-818`。

### F06 会话列表浏览/搜索/打开/重命名/删除（6 步）

**happy path**
1. Sidebar 挂载 → `useSessionList("global")` → `sessions.list("global")`；`isLoading`、失败置 `error`（`Sidebar.tsx:298`；`use-session-list.ts:29-75`）。
2. 搜索：输入框本地过滤；侧栏搜索按钮/⇧⌘F 打开全局会话搜索（消息级 `conversations:search`）（`Sidebar.tsx:1201-1208`；`main/ipc.ts:832-841`）。
3. 打开：点击行 → `openSessionFromSidebar(session)`：可复用的空/浏览态同 kind 同 cwd tab 就地切换，否则 `openTab({cwd, sessionPath, kind, browsing:true})`（`use-session-switch.ts:267-277`）。
4. 重命名：行内输入提交 → `renameSession(path,name)` → `sessions:rename`；主进程校验 `.jsonl`、非空名，live owner 走 `commandForIdleSession({type:"set_session_name"})`，否则用调用方 rpc（`use-session-list.ts:87-96`；`main/ipc.ts:651-679`）。当前会话也可 `RenameSessionDialog` → `rpc.setSessionName` + 本地 `sessionName` 同步（`RenameSessionDialog.tsx:42-48`）。
5. 删除：行内确认 → `deleteSession(path)`；若删的是当前会话 → `dropSessionNow()`（`drop_session`）；批量删除逐条并显示进度（`Sidebar.tsx:636-690`）。
6. `sessions:changed` 事件触发去抖刷新（`use-session-list.ts` 订阅）。

**边界**
- 读失败 ≠ 空：`error` 字段必须显示「读不到」而不是「没有会话」。
- 主进程列表过滤掉 scheduled run 的 session 文件（`main/ipc.ts:625-627`）。
- 删除 live owner：用 `drop_session`；运行中 `Session is currently running`；用户取消 `Session deletion was cancelled`；无进程的 owner 先 `noteSessionFile(tabId,null)` 再删文件（630-649）。
- 删除后本地行立即移除，再由后台刷新对账（`use-session-list.ts:79-85`）。

**证据**
- `renderer/components/layout/Sidebar.tsx:298-330,588-690,1201-1208`。
- `renderer/hooks/use-session-list.ts:29-99`。
- `renderer/hooks/use-session-switch.ts:267-291`。
- `renderer/components/dialogs/RenameSessionDialog.tsx:41-50`。
- `main/ipc.ts:622-679,822-841`。

### F07 恢复历史会话（⌘P）（5 步）

**happy path**
1. ⌘P / 侧栏搜索图标 → `SessionPickerDialog`（`ui.openSessionPicker`）。
2. 打开时按 scope 拉 `sessions.list`，local/global 结果缓存；本地 `rankSessions`（字面优先、再子序列模糊，支持 sort）→ 合并 `sessions.search(q, scope)` 的内容命中（`SessionPickerDialog.tsx:128-157`；`lib/session-search.ts`）。
3. 选中 → `select()`：
   - 文件已属于某 tab → `switchSessionNow` 先 `tabs.getSessionOwner` → `routeToSessionOwner`（切 tab 或聚焦 owner 窗口）；
   - 否则 `switchPending={fromId,toId}` → `rpc.switchSession(sessionPath[,model])`（`use-session-switch.ts:150-183`）。
4. 成功后 `hydrateSession` / `hydrateTabSession` 重灌所有 store（transcript、tools、model、settings、subagents、todo、extension-ui、plan-approval）。
5. 忙（streaming/compacting）时 `requestSessionSwitch` 先开 `SessionSwitchDialog`（新 tab / 新窗口 / 中断后切换），确认后再走同一路径（284-291）。

**边界**
- kind 不匹配：不在原地切，改开新 tab 并 toast（174-183）。
- `session_owned_elsewhere`：按 payload 的 ownerTabId/ownerWinId 路由（199-206）。
- `session_kind_mismatch`：报错并保持当前会话。
- 模型不可用：`parseModelRestoreFailure` → `ModelRestoreDialog`。
- 扩展 hook 否决：`success:true, cancelled:true` → 不切，info toast。
- 同 id / 空 id：直接返回。

**证据**
- `renderer/components/dialogs/SessionPickerDialog.tsx:36-216`。
- `renderer/hooks/use-session-switch.ts:126-235,284-291`。
- `renderer/lib/session-search.ts`。
- `main/ipc.ts:816-818`（`GET_SESSION_OWNER`）。

### F08 从消息 branch / fork（5 步）

**happy path**
1. 消息悬浮操作 → fork：`forkSessionFromMessageInNewTab(message, rpc, tabId)`（`MessageBubble.tsx:447-450`）。
2. 旧消息可能没有 `entryId`：`resolveMessageEntryId` 用 `get_session_tree` 按时间戳、文本预览、出现次序定位（`lib/messages.ts:130-170`）。
3. `rpc.forkFrom(entryId)` → 返回 `sessionPath` → 若活动 tab 已变则只保存并 warning，否则 `openTab({cwd,sessionPath,kind})` 打开新 tab（111-140）。
4. 编辑/回溯：`editFromMessage` → `rpc.branch(entryId)` → `cancelled` 提示，否则 `omp:fill-composer` 回填 + hydrate（189-208）。
5. Session 树对话框：`get_session_tree` 可用则分层图，否则 `get_branch_messages` 降级单链；节点动作 `switch_leaf`（Shift+Enter 先总结）、`branch`（仅 user 节点）、`fork_from` + `sessions.openInNewWindow`（`SessionTreeDialog.tsx:91-108,440-548,632-658`）。BranchPicker 是纯列表版：`getBranchMessages` → `branchSessionFromEntry`（`BranchPickerDialog.tsx:41-63`；`lib/messages.ts:98-108`）。

**边界**
- 非 user 节点隐藏 branch（服务端只允许从 user entry 分支）。
- `switch_leaf` 可返回 `cancelled` 或 `reopenAsk`，F-OWN 情况先让 owner 处理。
- fork 到「新窗口」失败 → toast；fork 到新 tab 失败 → 返回 `"saved"`。
- 会话正在切换（`switchPending`）时拒绝操作并报错。
- 摘要切换是一次模型调用，可能很久（`summarizingSince` + LongTaskStatus）。

**证据**
- `renderer/lib/messages.ts:98-208`。
- `renderer/components/chat/MessageBubble.tsx:440-465`。
- `renderer/components/dialogs/SessionTreeDialog.tsx:1-22,90-108,440-548`。
- `renderer/components/dialogs/BranchPickerDialog.tsx:1-63`。
- `shared/rpc-client.ts:108-109,138-140`（`branch`/`fork`/`fork_from`/`switch_leaf`）。

### F09 压缩（compact）与 handoff（4 步 / 6 步）

**compact**
1. `/compact` 或命令面板 → `planComposerSubmit` 返回 `{kind:"compact", request:()=>rpc.compact()}`（`lib/composer-submit.ts:36,121-124`）——专走 compact 超时，避免被 8s 的 prompt 超时打断。
2. 执行期间 `sending=true`；成功 `settleComposerResponse` + hydrate；服务端拒绝（太小/已压缩）→ warning toast，不回填草稿；`rpc_delivery_unknown` → 回填草稿 + 不确定标记（`use-composer-submit.ts` compact 分支）。
3. 自动压缩：`auto_compaction_start` → `isCompacting=true` + `compactionInfo{reason,action}`；`auto_compaction_end` → 清理；未 aborted 则 hydrate，aborted 则 warning toast（`use-rpc-events.ts:390-410`）。
4. 阈值/开关来自 `get_settings`（`use-compaction-config.ts` → `parseCompactionConfig`/`windowCompactionThreshold`），`ContextStrategyBar` 展示触发点。

**handoff**
1. 菜单/命令 → `openHandoffDialog()`（`stores/fork-handoff.ts:22`）。
2. 填可选指令；`blockedReason` = streaming 或 `messageCount===0`（`HandoffDialog.tsx:75-79`）。
3. `tabRpc.handoff(instructions?)`：`null` 结果 = agent 侧取消 → 关闭 + info toast；`stopRequested` 在摘要写完后不再 fork（93-108）。
4. `forkAttempted` 时用 `get_state` 确认 sessionId 未变，否则放弃并提示会话已变（110-120）。
5. `tabRpc.fork()`；`cancelled` → 报错；成功 → hydrate + success toast（含 savedPath）（125-136）。
6. 阶段机 `stage:"summary"|"fork"`，`LongTaskStatus` 显示耗时，Cancel 在运行中发 `abort`；fork 步骤不可取消（58-63）。

**边界**
- 运行中关闭对话框无效（`requestClose` 仅 `!running`）。
- handoff 保留当前会话，新会话携带摘要上下文。
- compact 可被 Stop（`LongTaskStatus onStop → tabRpc.abort()`）。

**证据**
- `renderer/lib/composer-submit.ts:20-40,99-137`。
- `renderer/components/layout/use-composer-submit.ts` compact 分支。
- `renderer/hooks/use-rpc-events.ts:390-410`。
- `renderer/hooks/use-compaction-config.ts`。
- `renderer/components/dialogs/HandoffDialog.tsx:1-205`。
- `renderer/stores/fork-handoff.ts:9-28`。
- `shared/rpc-client.ts:208-225`（`compact`/`handoff`/`fork`）。

### F10 plan mode 提案与审批（7 步）

**happy path**
1. 开 plan mode：composer `ComposerModes` 的 `togglePlan`（`ComposerModes.tsx:133`）；⌥⇧P（`App.tsx:475-484`）；`PlanDockCard`（`PlanDockCard.tsx:250-251`）；命令面板（`CommandPalette.tsx:382`）。全部走 `set_plan_mode`。
2. agent 在 plan mode 下把计划写到 `xd://propose` → sidecar 发 `plan_proposal` 帧，静默停止该 turn，等待 `plan_approval`（`PlanApprovalDialog.tsx:2-14` 注释）。
3. `usePlanApproval` 订阅 batch，把 proposal 放进 store（最新覆盖旧的）（`use-plan-approval.ts:17-42`；`stores/plan-approval.ts:51`）。
4. Dialog 渲染 `planContent`（Markdown）+ 由 `pending.options` 生成的批准按钮；`approveOptionsOf` 只认 `execute/compact/keep_context/save`，都不认识时回落 `execute`；`showRefine = options 为空或含 refine`（`PlanApprovalDialog.tsx:71-77,180-181`）。
5. 提交：approve execute/compact/keep_context → `planApproval(true, option)`；save → 先 `showSaveDialog` 再带 `savePath`；refine → `planApproval(false, undefined, feedback)`；dismiss/Esc → `planApproval(false)`（95-121）。
6. 结算：`settleTabPlanApproval(originTabId, originSessionId, target, {...})`；`dispatched:false` 时保留对话框并显示 `reason`；save 成功 toast savedPath / freshSessionStarted（124-150）。
7. accept 后服务端退出 plan mode 并派发执行 turn（无事件）；下一次 `agent_start` 时 `refreshSessionState` 把 planModeEnabled 等状态拉回（`use-rpc-events.ts:330-352` 注释）。

**边界**
- 未 ready 或正在提交 → 按钮 disabled；proposal latest-wins。
- save 未选路径 → 不提交。
- approve-and-compact 是长任务，可 Stop → `tabRpc.abort()`。
- tab 关闭后迟到的 `plan_proposal` 被 `closedTabFrame` 丢弃；sidecar `starting` 时 `clearProposal()`（`use-rpc-events.ts` status 分支）。
- 提交状态挂在发起 tab 上（`submitting`），切走再回来不会重复提交（`plan-approval.ts:31-32`）。

**证据**
- `renderer/stores/plan-approval.ts:12-63`。
- `renderer/hooks/use-plan-approval.ts:17-42`。
- `renderer/components/dialogs/PlanApprovalDialog.tsx:1-281`。
- `renderer/components/layout/ComposerModes.tsx:133`；`renderer/App.tsx:475-484`。
- `shared/rpc-client.ts:295-296`（`plan_approval`）。

### F11 权限确认（tool approval）审批（6 步）

**happy path**
1. agent 需要工具许可 → sidecar 发 `extension_ui_request`：`method:"select"`，`title` 以 `Allow tool: ` 开头，options 含 `Approve`、可含 grant 项、以 `Deny` 结尾（`shared/approval-prompt.ts:9-31`）。
2. 主进程推送 `extension-ui:request`；`useExtensionUi` 按 tabId `pushTabExtensionUiRequest` 入该 tab 的 store（`use-extension-ui.ts:17-25`；`stores/tabs.ts:908+`）。
3. `ExtensionDialog` 取跨 tab 聚合后的最旧一条；`isApprovalRequest` 为真 → `ApprovalDialog`，否则按 method 渲染 Select/Confirm/Input/askDialog/Editor/OpenUrl（`ExtensionDialog.tsx:644-720`）。
4. `ApprovalDialog`：解析 toolName/正文，按 READ/WRITE 工具集给 read/write/exec tier badge，`always-ask-cached` 下额外列出「记住此文件/文件夹」授权行（`ApprovalDialog.tsx:65-125`）。
5. 回答 → `window.omp.ui.respondExtensionUi({type:"extension_ui_response", id, ...})` → `extension-ui:respond` → 主进程 `routeSideChannel(id, response, true)` 送回**原 tab**（`ExtensionDialog.tsx:523`；`main/ipc.ts:591-598`）。
6. `removeExtensionRequest(id)` 从 store 移除，队列推进到下一条（`ExtensionDialog.tsx:524`；`stores/extension-ui.ts:160-163`）。

**边界**
- 每条带 `timeout` 的请求都有倒计时，超时自动 `cancel`；倒计时按 requestId 重置（避免上一条的 0ms 泄漏到下一条）。
- `BLOCKING_UI_METHODS = {select, confirm, askDialog, ...}` 驱动标题 `!` 前缀、sidebar 信号与 `progress:set("waiting")`（`shared/rpc-types.ts:1993`；`hooks/use-awaiting-confirmation.ts`；`App.tsx:FocusedSessionEffects`）。
- `notify/setStatus/setWidget/setTitle/set_editor_text/cancel` 不是对话框，由 store/router 消费（`ExtensionDialog.tsx:NON_DIALOG_METHODS`）。
- `open_url` 里等待粘贴 authCode 的 `input` 请求并入登录对话框（`belongsToOpenUrl`）。
- 非 focused tab 的请求先存在该 tab 的 store，切过去再显示（`useAllPendingRequests`）。

**证据**
- `renderer/components/dialogs/ExtensionDialog.tsx:1-90,523-810`。
- `renderer/components/dialogs/ApprovalDialog.tsx:1-150`。
- `renderer/hooks/use-extension-ui.ts:12-38`；`renderer/stores/extension-ui.ts:12-79,152-163`。
- `shared/approval-prompt.ts:1-40`；`shared/rpc-types.ts:1993`。
- `main/ipc.ts:591-620`。

### F12 子代理 spawn 与 DAG 视图（6 步）

**happy path**
1. 主 agent 调 `task` 工具 → sidecar 起子代理（独立 session）；渲染层收到 `subagent_frame`，或按需 `get_subagents`（`stores/subagents.ts:75-130`；`hooks/use-subagent-stream-poll.ts:18-35`）。
2. store 合并：lifecycle `started` → `running`；progress 帧更新 status/description/task；`running && live===false` 归一为 `stale`（`subagents.ts:52-54,90-130`）。
3. 入口：Workspace dock 的 `AgentsDockCard`（`WorkspaceDock.tsx:28,146`）与右侧面板 `agents` tab 的 `AgentRoster`（`PanelContainer.tsx:215`）。
4. Agent Hub 窗口：`definitions`（agent 定义、模型覆盖、启停）与 `hub`（运行表：状态/模型/耗时、abort、revive、暂停全部、abort turn）（`AgentHubWindow.tsx:60,984-1022`）。
5. `hub` 行 → transcript 抽屉（`SubagentTranscript`，`get_subagent_messages` 字节分页）（672-713）。
6. DAG：`SubagentDag` 以主会话为合成根，按 `parentToolCallId`/`parentSubagentId` 建边，分层 tidy 布局；点节点选中并在下方看 transcript（`SubagentDag.tsx:136-240`；`stores/subagent-graph.ts`）。

**边界**
- 服务端刷新时「忘记」终态行：本地保留；live 行刷新缺失则丢弃（`subagents.ts:36-41`）。
- advisor 子代理只读：abort 返回 `advisor_readonly`；主 agent 返回 `main_agent`；非 parked 返回 `not_parked`（`AgentHubWindow.tsx:806-824`）。
- 轮询只在 streaming 或已有 agent 时开启，且带 tab guard（切 tab 后不写错 store）。
- DAG 未解析父边进 `layout.unresolved` 并提示（`SubagentDag.tsx:271-280`）。
- 订阅等级固定 `progress`（`subagents.ts:12`），由 `set_subagent_subscription` 下发。

**证据**
- `renderer/stores/subagents.ts:12-130,182-202`。
- `renderer/hooks/use-subagent-stream-poll.ts:1-36`。
- `renderer/components/panels/SubagentDag.tsx:1-136,164-280`；`renderer/stores/subagent-graph.ts`。
- `renderer/components/panels/AgentHubWindow.tsx:1-60,530-1025`。
- `renderer/components/chat/dock/AgentsDockCard.tsx`；`renderer/components/panels/SubagentTranscript.tsx`。

### F13 todo / queue dock（5 步）

**happy path**
1. `todo` 工具结果 → `todoPhasesFromToolResult(result)` → `setPhases(phases)`；首次为 hydrate 不入历史，之后状态变更就地更新最后一条快照，列表变更追加（`stores/todo.ts:117-140,161-171`）。
2. `todo_reminder` → `showReminder`；`todo_auto_clear` → 清 live 保留完成快照（`use-rpc-events.ts` 对应 case）。
3. `TodoDockCard`：无数据返回 null；有数据时按 phase 折叠/全量、勾选、拖拽排序；写入走 `rpc.setTodos(payload)`，乐观更新失败回滚 toast（`TodoDockCard.tsx:300-372`）。
4. 队列：`queue_update` 帧 → `setFromFrame({steering,followUp})`（`stores/queue.ts:45-48`）；挂载/重连时 `get_queue` 兜底刷新（`useQueuedMessages`）。
5. `QueueDockChip` 显示两 lane 计数，点开 Modal → `QueuePanel`（`QueueDockChip.tsx:16-69`）；面板内拖拽 `queue_move`、跨 lane `queue_move(toLane)`、`queue_remove`、`queue_clear`，乐观应用后 `get_queue` 重同步（`QueuePanel.tsx:1-60`）。

**边界**
- 无 todo/queue → 两个组件都不渲染。
- 输入侧：`->`/`=>` 简写拆成多条；首条在空闲时以 `followUp` prompt 启动，其余 `follow_up`；部分失败把剩余项还原成简写草稿（`use-composer-submit.ts` queueBody 分支）。
- ⌥↑ `dequeue`：最新一条回填 composer（prepend），其余按原 lane 重排（`lib/messages.ts:226-250`；`App.tsx` `dequeue`）。
- 扩展命令文本不能走 `follow_up`（服务端抛错），改走 `prompt`（源码注释）。
- lane 顺序以服务端 clamp 为准，客户端乐观值随后被刷新覆盖。

**证据**
- `renderer/stores/todo.ts:5-40,117-189`；`renderer/components/chat/dock/TodoDockCard.tsx:300-450`。
- `renderer/stores/queue.ts:16-63`；`renderer/components/panels/QueuePanel.tsx:1-73`；`renderer/components/chat/dock/QueueDockChip.tsx:1-70`。
- `renderer/hooks/use-rpc-events.ts`（`todo_reminder`/`todo_auto_clear`/`queue_update`）。
- `shared/rpc-client.ts:113-120`（`dequeue`/`get_queue`/`queue_*`）。

### F14 文件面板浏览 + diff 视图（6 步）

**happy path**
1. 右侧面板 `PanelContainer`，tab `files` → `FilesPanel`（`PanelContainer.tsx:34-42,218`）。
2. `fs:list(undefined, depth=8, max=2000, tabId)` → 树；`ok:false` 显示错误空态（`FilesPanel.tsx:98-110`；`main/ipc.ts:1151-1185`）。
3. 点文件 → `fs:read(path, 200_000, tabId)` 预览；`openFilePreview(path,line)` 同时切到 files tab（`FilesPanel.tsx:134-140`；`stores/ui.ts:344-345`）。
4. `@` 按钮 → `omp:insert-mention` 事件 → `InputArea` 监听到并追加 `@path`（`FilesPanel.tsx:174-176`；`InputArea.tsx:346-353`）。
5. `diff` tab → `DiffPanel` 从 `useToolsStore` 取 `edit/apply_patch/ast_edit/write` 的结果，多文件下拉选择、统一/分栏切换（`DiffPanel.tsx:51-62`；`stores/ui.ts:337-341` 的 `diffScope`）。
6. Timeline 视图：本会话全部编辑操作按文件分组、累计 +/- 与活动条（`DiffPanel.tsx:1-14`，虚拟列表）。

**边界**
- 树上限 2000 项、深度 8（`FS_LIST_MAX_DEPTH=16` 但 GUI 传 8），截断标记 `truncated`。
- 无工作区 → `ok:false, error:"No workspace"`（`main/ipc.ts:1152-1153`）。
- 二进制/超大文件：`fs:read` 返回 `binary`/`truncated`；预览最多 200k 字符。
- 快速切文件：`listVersion`/`previewVersion` 丢弃过期响应。
- 没有编辑结果 → 空态；chat tab 只显示 files（无 diff）。

**证据**
- `renderer/components/panels/FilesPanel.tsx:1-200`。
- `main/ipc.ts:1151-1318`（`FS_LIST`/`FS_READ`/`FS_READ_IMAGE`/`FS_READ_PLAN`）。
- `renderer/components/panels/DiffPanel.tsx:1-120`；`renderer/lib/diff.tsx`。
- `renderer/components/layout/PanelContainer.tsx:34-42,72-75,213-220`。
- `renderer/stores/ui.ts:48,337-345`。

### F15 内置浏览器 open/navigate/capture（6 步）

**happy path**
1. 面板 tab `browser`（chat tab 不可见）→ `BrowserPanel` 挂载；`window.omp.browser.getState(tabId)` 拉当前状态并 `applyBrowserState(state,{reveal:false})`（`BrowserPanel.tsx:172-177`；`stores/browser.ts:22-38`）。
2. 地址栏/链接 → `window.omp.browser.open(tabId,url)` → `main/ipc.ts:776-780` → `BrowserHost.open` 创建页面并返回新状态（`browser-host.ts:1025-1029`）。
3. 布局：ResizeObserver 计算面板矩形（去重 key 后）→ `browser:set-viewport({tabId,bounds})`；不可见/卸载 → `bounds:null`（`BrowserPanel.tsx:225-251`；`main/ipc.ts` `BROWSER_SET_VIEWPORT`；`browser-host.ts:1043-1049`）。
4. 导航工具栏：back/forward/reload/stop/navigate/external/select/close/devtools/find/stopFind → `browser.action({tabId,pageId,action,...})`，响应再 `applyBrowserState`（`BrowserPanel.tsx:255-274,432-468`；`main/ipc.ts:129-141,781-797`）。
5. 遮挡：对话框/菜单/toast 与面板相交 → `browser.capture(tabId)` 取静帧替换 live view；解除后恢复 live（`BrowserPanel.tsx:201-215`；`main/ipc.ts:798-800`）。
6. agent 侧：sidecar `browser_host_start` 起私有 relay；`BrowserHost` 以扩展身份连接，逐页 attach CDP；agent 最后一次命令后 30s 休眠，20s ping（`browser-host.ts:27-30,75-76,858-870,978-1000`）。

**边界**
- 同一窗口只显示一个 tab 的视口：设置新 tab 时其它 tab 的视口置 null（`setViewport` 循环）。
- 页面加载失败 → `IpcBrowserPage.loadError`；下次导航清空。
- popup 被接管为面板页（`setWindowOpenHandler` 返回 `allow + createWindow` 路径，`browser-host.ts:664-670`）。
- 聚焦页面时应用快捷键：`browser:set-shortcuts(chords)` + `browser:key` 回放（`App.tsx` 两个 effect）。
- stock sidecar 无 `browser_host_start` → agent 用自带浏览器；GUI 仍可用面板（relay 未链接时 `agentLinked:false`）。

**证据**
- `renderer/components/panels/BrowserPanel.tsx:1-73,172-274,425-470`。
- `renderer/stores/browser.ts:8-43`。
- `main/ipc.ts:129-141,773-806`。
- `main/browser-host.ts:14-30,74-79,229-311,664-670,858-870,978-1049`。
- `shared/ipc-types.ts:974-1000`（`IpcBrowserPage`/`IpcBrowserState`）。

### F16 终端打开与运行（5 步）

**happy path**
1. 面板 tab `terminal` → `TerminalPanel`；首次 `syncTerminals(tabId)` → `terminal_list`（`TerminalPanel.tsx:152-160`；`stores/terminals.ts:253-262`）。
2. 该 tab 无终端且未自动开过 → `openTerminal(tabId)` → `terminal_open`（可选 cols/rows）→ `RpcTerminalInfo`；`createView` 建 xterm + FitAddon 并挂到容器（`TerminalPanel.tsx:138-170`；`terminals.ts:281-288,145-162`）。
3. `terminal_snapshot` 回放既有输出，之后按 `seq` 续接（`terminals.ts:232-237`；`shared/rpc-types.ts:812-816`）。
4. 输入/尺寸：`term.onData`/`onBinary` → `terminal_input`；`onResize` → `terminal_resize`（`terminals.ts:153-160`）。
5. 帧循环：`terminal_opened`（可带 `reveal`）/`terminal_output`/`terminal_exit`/`terminal_closed` 更新 store 与视图；关闭 → `terminal_close`，帧回来移除（`terminals.ts:200-225,291-298`）。

**边界**
- 未见过 open 的 output 帧 → 触发一次 `syncTerminals` 拉快照（`terminals.ts:211-218`）。
- `running:false` + `exitCode` 的终端仍留在列表，直到关闭。
- tab 关闭 → `forgetTabTerminals` 释放所有视图（`terminals.ts:301-305`）。
- 主题切换 → `applyTerminalTheme()` 重写每个 xterm theme。
- agent 的 `pty:true` bash 也会出现在同一面板（`kind:"command"`，`reveal` 要求 GUI 显示）。

**证据**
- `renderer/stores/terminals.ts:26-305`。
- `renderer/components/panels/TerminalPanel.tsx:1-220`。
- `shared/rpc-types.ts:740-816`。
- `shared/rpc-client.ts:212-213`（`bash`/`abort_bash` 与终端命令并列）。

### F17 定时任务创建/运行/复核（7 步）

**happy path**
1. 菜单/命令面板/侧栏 → `useSchedulerStore.openTasks(target?)`（`stores/scheduler.ts:122-130`；`App.tsx` `open-scheduled-tasks`）。
2. `TasksWindow` 打开，4 个视图 tabs：tasks/runs/review/insights，runs 与 review 带 badge（`TasksWindow.tsx:64-118`）。
3. New → `TaskEditorDialog` + `TaskForm`；表单改动 200ms 去抖调 `scheduler:validate`，返回 `DraftProblem[]`（`TaskForm.tsx:28-40,501+`）。
4. 保存 → `scheduler:create`（新）或 `scheduler:update`（configOf(draft)）；`ok:false` 显示 problems；成功关编辑器并落到 tasks 视图（`scheduler.ts:145-152`）。
5. 运行：`runNow(taskId)` → `scheduler:run-now` → 主进程 `Scheduler.runNow` → `#fire(task,"manual",null)`；定时触发按 `cron|interval|once`（`scheduler.ts:153-158`；`main/scheduler/scheduler.ts:326-330`；`shared/scheduler-types.ts:7-10`）。
6. 进度：`scheduler:progress(runId, progress)` 更新 store；snapshot 推送时丢弃已结束 run 的 progress；结束刷新 notices（`scheduler.ts:92-115`）。
7. 复核：Review 只列 `review.state==="pending"`；accept/reject（可带 note）→ `scheduler:review`；reject 且有 worktree → 清理 worktree + branch（`RunsView.tsx:136-226`；`main/scheduler/scheduler.ts:380-390`）。

**边界**
- Draft 校验项：`title_missing/prompt_missing/cwd_missing/schedule_invalid/schedule_in_past/too_frequent/depth_invalid/timeout_invalid/verify_invalid`（`TaskForm.tsx` `problemText(...)`；`shared/scheduler-types.ts:182+`）。
- 运行重叠 → `skipped_overlap`/`skipped_backlog`；配置阻塞 → `blocked_config`（`RunState`）。
- 取消：queued 直接出队；运行中取消写 `cancelReason`（`scheduler.cancelRun`）。
- 应用退出取消运行（`cancelReason:"app_quit"`）。
- 超时（`timeoutMs`）→ `timed_out`；失败连击 → `disabled` + `failure_streak`。
- 全局暂停 / 健康 `healthy|paused|stalled` 显示在窗口头部。

**证据**
- `renderer/stores/scheduler.ts:21-207`。
- `renderer/components/tasks/TasksWindow.tsx:1-121`；`TaskForm.tsx:25-40,501-660`；`RunsView.tsx:1-226`；`InsightsView.tsx`。
- `main/scheduler/scheduler.ts:171-222,326-394`；`main/scheduler/manage-tasks-tool.ts`。
- `shared/scheduler-types.ts:7-18,26,60-104,160-217`。

### F18 统计看板（5 步）

**happy path**
1. 菜单/命令 `open-stats` → `ui.openStatsDashboard()` → lazy `StatsDashboard`（`App.tsx`；`StatsDashboard.tsx:42`）。
2. 打开即静默 sync：`runStatsSync(path => window.omp.stats.fetch(path))` → `POST /api/sync`；失败时 `unavailable` 不弹错（`StatsDashboard.tsx:48-70`；`lib/stats-sync.ts`）。
3. 各路由用 `useStats('<path>', params)` → `stats:fetch` → 主进程 `StatsClient` 白名单转发（`hooks/use-stats.ts:61`；`main/ipc.ts:844-870`）。
4. 6 个 range（`1h/24h/7d/30d/90d/all`）与 10 个路由，切换即带 `refreshKey` 重拉（`StatsDashboard.tsx:24-38,167-176`）。
5. 服务端仍在启动 → 保持 loading 快速重试；其它错误显示 message 并可 `refetch`（`use-stats.ts:60-100`）。

**边界**
- `stats:fetch` 非白名单路径 → throw `Invalid stats path`（`main/ipc.ts:845-847`）。
- `stats:url` 在端口丢失时触发 revive；`exhausted` 返回 `{url:null, unavailable:false}`（`main/ipc.ts:872-878`）。
- 列表型接口（`useStatsList`）返回非数组视为错误，不渲染空表（`use-stats.ts:115-117`）。
- 页面隐藏时不刷新，重新可见刷新（`use-stats.ts:89-96`）。

**证据**
- `renderer/components/stats/StatsDashboard.tsx:1-181`；`renderer/components/stats/{Overview,Models,Providers,Tools,Costs,Errors,Frustration,Gain,Projects,Requests}Route.tsx`。
- `renderer/hooks/use-stats.ts:44-117`；`renderer/lib/stats-sync.ts`。
- `main/ipc.ts:844-878`。

### F19 设置 schema 编辑（6 步）

**happy path**
1. ⌘, 或菜单 → `ui.openSettings(tab?)`（默认 `gui`）（`App.tsx`；`stores/ui.ts:366-370`）。
2. 打开时并行 `tabRpc.getSettingsSchema()` + `getSettings()`；失败 → `schema=null` 并显示错误、可重试（`SettingsWindow.tsx:285-300`）。
3. 渲染：schema 的 tabs/groups/labels/control types 驱动；无 UI 元数据进 Advanced；以 Model/Provider 结尾的字符串字段换成搜索下拉（`SettingsWindow.tsx:9-22`；`SchemaTabContent.tsx:26-49`）。
4. 改一行：`SchemaSettingRow.commit(next)` → `rpc.setSetting(entry.path, next)`（`SchemaSettingRow.tsx:164-205`）。
5. 成功：有 `provenance` 用返回的 `value`，否则再 `getSettings` 读回 effective value；失败保留 draft 并显示错误（169-190）。
6. GUI 本地项（密度、字号、通知、tab 宽度等）走 `prefs:set`，App 启动时 `prefs:get` 水合（`App.tsx` boot effect；`main/ipc.ts:1022-1063`）。

**边界**
- 并发写：`generation` 递增，过期响应不落地（166-171）。
- 「恢复默认」提交 `undefined`（627-630）。
- JSON 编辑解析失败 → 不提交，仅显示错误。
- `set_setting` 成功但无 provenance（旧 core）→ 回读校验，读不到报 `settings.savedUnverified`。
- sidecar 非 ready → 行禁用；schema 拉不到时 GUI 页仍可用。

**证据**
- `renderer/components/settings/SettingsWindow.tsx:1-83,285-300`；`SchemaSettingRow.tsx:105-224,627-630`；`pages/SchemaTabContent.tsx:20-49`。
- `renderer/stores/settings.ts:7-52,199-251`。
- `main/ipc.ts:1022-1063`（`PREFS_GET/SET/UPDATE_LAUNCH_PROFILE`）。
- `shared/rpc-client.ts` `getSettingsSchema`/`getSettings`/`setSetting`。

### F20 主题切换（5 步）

**happy path**
1. 命令/设置/主题按钮 → `ui.openThemePicker()`（`stores/ui.ts:403`）。
2. `ThemePickerDialog` 列内置 `THEMES`、`system`、`~/.omp/gui/themes` 自定义主题；不可用的自定义主题带原因（`ThemePickerDialog.tsx:1-9`；`lib/themes.ts:1696-1726`）。
3. 搜索/键盘高亮 → 选择卡片。
4. 应用：`applyThemeByName(selection)` 把整套 `--omp-*` token 写成 `<html>` inline 变量并持久化，同时同步 `data-theme`/`color-scheme`（`themes.ts:1791-1850`；`lib/theme.ts`）。
5. `setTheme(entry.theme?.scheme ?? "system")` 保持旧 dark/light store 一致，toast + 关闭（`ThemePickerDialog.tsx:143-148`）。

**边界**
- 启动首帧：`prefs.themeName/theme` → `getPersistedThemeSelection` → `applyThemeByName(sel,{persist:false,remember:true})`；`pre-paint.js` 用记住的值先上色（`App.tsx` boot effect；`themes.ts:1851`；`public/pre-paint.js`）。
- 自定义主题文件坏了：列表显示 `skipped` 原因，选择回落到系统方案（`resolveThemeSelection`）。
- `followAgentTheme && sidecar ready` → `initAgentThemeSync()` 跟随 agent `theme.*` 设置（`App.tsx`；`themes.ts:2221+`）。
- 插件主题是会话级的：tab 路由未 settle 时 `clearPluginThemes()`，settle 后 `refreshPluginThemes()`（`App.tsx` 两个 effect）。
- 系统主题变化只在 `theme==="system"` 时重新应用（`watchSystemTheme`，`App.tsx`）。

**证据**
- `renderer/components/dialogs/ThemePickerDialog.tsx:1-160`。
- `renderer/lib/themes.ts:36,1696-1726,1761-1860,2115-2230`；`renderer/lib/theme.ts`。
- `renderer/App.tsx` boot prefs / `initCustomThemes` / agent theme sync effects；`renderer/stores/custom-themes.ts`；`renderer/public/pre-paint.js`。
- `renderer/stores/ui.ts:70,271,402-404,557`。

### F21 命令面板 / slash 命令（6 步）

**happy path（⌘K）**
1. ⌘K → `ui.openCommandPalette()`（`App.tsx` keymap `palette`）。
2. 打开时读最近 5 条（`localStorage["omp.palette.recent"]`）并 `getAvailableCommands()`；失败保留原生命令并显示可重试的错误条（`CommandPalette.tsx:43,238-255`）。
3. 输入模糊过滤、按 category 分组；选中 `submenu` 项进入子菜单并清空查询（451-455,503-521）。
4. Enter/点击 → `execute(item)` → `runAffordance`：`action` 直接跑并关面板；`toggle` 取反；`picker`/`window` 打开；`prompt` 走 `rpc.prompt`（107-146）。
5. 错误 → `toast({variant:"error",title:t("palette.failed")})`；成功项写入最近列表。
6. 未 ready 时仅 `new-tab/new-chat-tab/close/quit` 与非 action 项可用（49-64）。

**happy path（slash）**
1. composer 里以 `/` 开头的文本 → `planComposerSubmit`。
2. `/clear`（精确）→ `clear_context` RPC；`/compact` → `compact` RPC；GUI-only 内置（`share/live/btw/delete/drop` 及 `textModeExecutable:false`）→ 本地 affordance，绝不作为文本发给模型；`/new`、`/clear` 等会话替换命令在流式中被阻断（`lib/composer-submit.ts:23-96,99-137`）。
3. 其余 slash 原样 `rpc.prompt`，由服务端解析（文本模式命令回 `command_output` 帧）。

**边界**
- `unavailable` affordance → warning toast 带 reason。
- 命令列表刷新事件 `commands:update`；InputArea 也订阅（`InputArea.tsx:275-300`）。
- 最近项去重/截断到 5（`RECENT_LIMIT`）。
- 子菜单里 Escape 返回主列表，再 Escape 关闭。

**证据**
- `renderer/components/dialogs/CommandPalette.tsx:20-64,107-146,211-255,451-575`。
- `renderer/lib/command-registry.ts:68-110,260-300,1330-1340,2001-2082`。
- `renderer/lib/composer-submit.ts:1-137`；`renderer/lib/command-availability.ts`。
- `renderer/components/layout/InputArea.tsx:270-305`。

### F22 @file 提及（4 步）

**happy path**
1. composer 输入 `@`（行首或空白后）→ `useCompletionMenu` provider 链：slash-arg → github-ref → slash names → @mention → emoji，首个有结果者胜（`use-completion-menu.ts:2-5,87-90`）。
2. @ provider：工作区文件模糊匹配（`filePaths`，`MAX_MENTION_FILE_ITEMS=20`）在上，内部 scheme（`skill:// memory:// artifact:// issue:// pr:// local://plan.md`）在下（226-243；`input-area-utils.ts:13-16`）。
3. 文件列表来自 `fs:list` 的 `listMentionFiles(cwd, tabId)`：按 cwd 缓存、inflight 去重、失败不缓存（54-75）。
4. 选中 → `applyCompletion` 用 provider 的 range 替换，插入 `@path `；`applyCompletion` 在光标不等于 rangeEnd 时放弃（`use-completion-menu.ts:44-52`）。含空格路径写作 `@"…"`（`droppedFileMention`，`input-area-utils.ts:104-109`）。

**边界**
- 文件读取失败 → 只显示 scheme 项，下次重试。
- 光标移动/继续打字 → 不替换（防覆盖）。
- chat tab 不出现模型 mention；`^selector` 模型委派是另一 provider（250-260）。
- 从 FilesPanel 插入走 `omp:insert-mention` 事件，与手输 `@` 汇合（`FilesPanel.tsx:174-176`；`InputArea.tsx:346-353`）。

**证据**
- `renderer/components/layout/use-completion-menu.ts:10-52,87-90,225-260`。
- `renderer/components/layout/input-area-utils.ts:12-16,28-75,100-115`。
- `renderer/components/panels/FilesPanel.tsx:168-180`；`renderer/components/layout/InputArea.tsx:310-353`。
- `renderer/lib/model-mentions.ts:1-45`。

### F23 语音听写（6 步）

**happy path**
1. 点麦克风（或快捷键）→ `useDictation.toggle()` → `new DictationSession(...).start()`：**立即**开麦，同时 `dictation_start`（`use-dictation.ts:38+`；`lib/dictation.ts:107-118`）。
2. 音频：`openBrowserMic` 用 `dictation-worklet.js` 每 100ms 产出 16kHz PCM16，`pcm16ToBase64` 后 `dictation_audio`（`dictation.ts:74-78,288-317`）。
3. 帧：`host_dictation_state`（recording/transcribing/idle）切换相位；`host_dictation_download` 进度；`host_dictation_text` 的 `volatile/clear/commit/trim/submit` 直接编辑 composer 光标处（147-186）。
4. `DictationStrip` 显示相位、电平、partial 文本；Esc 取消（`InputArea.tsx:944-945`；`DictationStrip.tsx`）。
5. 结束：`stop()` → `dictation_stop`；transcribing 完成后 final 词落位；`submit` 在 capture 结束后触发一次 `send(text)`（`dictation.ts:122-145`；`use-dictation.ts` 的 `sendRef`）。
6. 取消/卸载/切 tab → 停麦、关 AudioContext、清视图（`#closeMic`、effect cleanup）。

**边界**
- 麦克风权限被拒 / AudioContext suspended → `#fail` + toast（`openBrowserMic` catch）。
- `dictation_start` 返回 `state:"idle"`（已取消）→ 直接结束。
- 模型下载中关闭麦克风，避免积压陈旧音频；下载完成后提示再按一次（158-165）。
- transcribing 阶段再点 = 只结束，不再开新 capture（`stop()` 相位判断）。
- 插词间距由 `insertDictated` 处理（行首/空白后不补空格）；`trim` 处理口述「删除」。
- 只有 focused tab 的听写帧可达（按 tab 订阅）。

**证据**
- `renderer/lib/dictation.ts:11-60,74-200,285-348`。
- `renderer/components/layout/use-dictation.ts:1-133`。
- `renderer/components/layout/InputArea.tsx:534-537,940-950,1040-1075`。
- `renderer/components/layout/DictationStrip.tsx`；`renderer/public/dictation-worklet.js`。

### F24 更新流程（6 步）

**happy path**
1. 启动 3s 后一次、之后每 4h 自动 `checkForUpdates`（被动，不弹错）；手动 → `updater:check`（`main/updater.ts:318-336,254-266`）。
2. 事件 → 状态：`checking`；`available{version,notes,mode}`；`not-available{version}`（212-232）。`mode` = `automatic`（electron-updater）或 `manual`（macOS 架构安装包）。
3. 渲染层 `subscribeUpdaterStatus()`：订阅 `updater:status` + boot `getStatus()`（`stores/updater.ts:84-91`；`App.tsx:634`）。
4. Banner `available` → Download → `updater:download`；自动模式 `autoUpdater.downloadUpdate()`，manual 模式应用内下载并持续报 `downloading{percent,transferred,total}`（`UpdateBanner.tsx:24-69`；`main/updater.ts:267-292`）。
5. `downloaded{version,mode}`：自动 → "Restart" → `updater:apply` → `quitAndInstall()`；manual → "Open installer" 重新打开已下载安装包（`UpdateBanner.tsx:71-97`；`main/updater.ts:294-315`）。
6. `error{message,showInBanner}` → banner 显示 message + Retry（`updater:check`）/关闭；dismiss 按版本或 error 持久化，之后任何 `available/not-available/downloaded` 会重新武装（`stores/updater.ts:20-80`；`UpdateBanner.tsx:99-117`）。

**边界**
- macOS manual：`update-available` 里选架构资产；缺架构 → error `updates.unsupportedArchitecture`；缺安装包 → `updates.installerMissing`（`main/updater.ts:220-229`）。
- 被动轮询失败 `showInBanner:false`，不打扰；显式 IPC 失败才 `showInBanner:true`（248-252,261-266）。
- 检查无结果 → `settleIncompleteUpdateCheck` 收敛为 not-available / idle（254-259,321-328）。
- 旧版本遗留的部分下载文件在启动时清理（200-210 注释）。
- 手动安装模式下载失败不会自动重试。

**证据**
- `main/updater.ts:47-61,153-180,200-336`。
- `renderer/stores/updater.ts:14-91`；`renderer/components/layout/UpdateBanner.tsx:1-120`。
- `shared/ipc-types.ts:128-129,330-344`（`UpdateStatus`/`UpdateInstallMode`）。
- `main/ipc.ts`（updater 相关 handler 在 `main/updater.ts` 内注册）。

### F25 深链 `omp://`（4 步）

**happy path**
1. OS 拉起 `omp://new` 或 `omp://session/<id>`：Windows/Linux 交给第二实例的 argv（`app.on("second-instance")`），macOS 走 `open-url`（`main/deep-link.ts:21-23,58-60`）。
2. 协议注册：打包时声明 `omp` scheme，运行时 `app.setAsDefaultProtocolClient(PROTOCOL[, execPath, argv1])`（44-48）。
3. 解析：`new URL()` 后按 host 判定——`new` → `{action:"new-session"}`；`session` + pathname → `{action:"switch-session", sessionId}`；其它返回 null 丢弃（97-110）。
4. 发送到渲染层（`IPC_EVENTS.DEEP_LINK`）→ `App` 的 `onDeepLink`：
   - `new-session`：若 `isStreaming` → warning toast 不打断；否则 `newSessionNow()`；
   - `switch-session`：`sessions.list("global")` 按 id 找 → 找不到 error toast；找到 → `requestSessionSwitch(target)`（忙时进切换对话框）（`App.tsx:380-410`）。

**边界**
- 首个实例 handler 未注册 → URL 进 `beforeSetup` 队列，注册后回放（21-23）。
- 未知 host / 空 sessionId → 忽略（108-110）。
- 同 id 已打开 → `requestSessionSwitch` 直接 return。
- 跨 kind → 新 tab；会话已被别的 tab/窗口持有 → 路由到 owner。
- 新窗口场景：窗口创建时带 `pendingSession`，渲染层 boot 时 `sessions:consume-pending` 取一次再 `switch_session`（`main/ipc.ts:682-703`；`use-rpc-events.ts` ready 分支）。
- 启动 argv 同时带目录与深链 → 深链优先（`launch-argv.test.ts:17-21`）。

**证据**
- `main/deep-link.ts:1-115`；`main/launch-argv.test.ts:17-21`。
- `renderer/App.tsx:380-410`。
- `shared/ipc-types.ts` `DEEP_LINK`/`DeepLinkPayload`；`renderer/stores/ui.ts` 的 `requestSessionSwitch`。
- `main/ipc.ts:682-703`。

### F26 自动重试与模型回退（5 步）

**happy path**
1. 一轮失败 → sidecar 发 `auto_retry_start{attempt,maxAttempts,delayMs,errorMessage}`（`use-rpc-events.ts:412-437`）。
2. store：`retryPendingTabs.add(tabId)`、`retryInfo` 写入、`awaitingModelSince=null`；`TurnStatusRow` 渲染倒计时行（`ChatStream.tsx:1168-1200`）；屏外 tab 额外 warning toast。
3. 重试期间 `agent_end` 的中间帧（`isTerminal===false`）与通知都被门禁静默（`notifyOnAgentEnd`/`toastOnAgentError`）。
4. `auto_retry_end{success,finalError}` → 清 `retryInfo`；成功且仍在流式 → 重设 `awaitingModelSince`；屏外给 success / error toast（`use-rpc-events.ts` `case "auto_retry_end"`）。
5. 模型回退：`retry_fallback_applied{from,to}` → info toast；`retry_fallback_succeeded{model}` → success toast；`model_changed` → `refreshModelState`（510-512）。

**边界**
- 用户中断：`abortActiveTurn` 先 `abort_retry` 再 `abort`（`lib/messages.ts:80-95`）。
- 重试成功可能不发新的 `turn_start`：代码在 `auto_retry_end` 主动重设等待标记（`use-rpc-events.ts` 注释）。
- `agent_end` 可能在整个重试 saga 中途到达，也可能完全不到（注释）；全部按 tab 门禁。
- `agent_start` 重新进入时清 `retryInfo`，把界面交给真实流（330-340）。
- `auto_retry_end` 失败后再失败 → 错误卡 + Retry 按钮，而不是再发通知。

**证据**
- `renderer/hooks/use-rpc-events.ts:64-90,370-470`。
- `renderer/components/chat/ChatStream.tsx:1146-1230`。
- `renderer/stores/session.ts:47-62`。
- `shared/rpc-client.ts:102-104,210-211`（`abort`/`abortAndPrompt`/`abortRetry`）。
- `shared/rpc-types.ts` 的 `auto_retry_start/auto_retry_end/retry_fallback_*` 事件定义。

## 状态机

以下均为代码里**显式枚举/显式迁移**的状态机（非推断）。

### M1 sidecar 连接状态
- 状态：`asleep | starting | ready | exited | error | restarting`（`shared/rpc-types.ts:2611`，注释区分 asleep/exited/starting）。
- tab 级复合：`TabStatus = SidecarStatus | "running"`（`shared/ipc-types.ts:864`）。
- 迁移触发：主进程 spawn/崩溃/退避重启 → `tab:status`/`sidecar:status` 推送；渲染层 `useRpcEvents` 的 `handleStatus`：`starting` 全量 reset 各 store，`ready` 起心跳 + 一次性健康检查 + `consumePendingOpen` + hydrate，`error/exited/restarting` 停心跳。
- UI 消费：`SidecarBanner`、`TitleBar`、`InputArea` 的 `status !== "ready"` 门禁、`Progress`（`working|waiting|idle`，`ipc-types.ts:404`）。

### M2 tab 运行/通知状态
- `SessionTab.status: TabStatus`（`renderer/stores/tabs.ts:56-91`）。
- 附加布尔：`unreadDone`（后台完成）、`attention: "retrying" | "failed"`、`compacting`、`browsing`、`pendingSessionPath`。
- 迁移：`applyTabStatus(payload)` 合并 `TAB_STATUS`；`agent_start` 时 `setBrowsing(tab,false)`；`agent_end` 后台 settle → `notifyBackgroundSettle`。

### M3 工具调用状态
- 状态：`pending | running | done | error | aborted`（`renderer/stores/tools.ts:14`）。
- 合法迁移：`pending → running → done|error`；`pending|running → aborted`（历史重建，`tools.ts:188,205`）；`aborted → running`（迟到 `tool_execution_start` 复用占位卡，`114-118`）。
- 视图派生：`pending` 视觉当 running（spinner），`aborted` warning，`isError` error（`ToolCard.tsx:51-56`）。

### M4 回合/运行状态（session store）
- 字段与语义（`renderer/stores/session.ts:31-83`）：
  - `isStreaming`：`agent_start → true`，`agent_end → false`。
  - `awaitingModelSince`：`turn_start` 置 now；`message_end(assistant)`/`turn_end` 清空；重试成功重设。
  - `retryInfo{attempt,maxAttempts,delayMs,errorMessage,startedAt}`：`auto_retry_start → 写入`，`auto_retry_end → null`。
  - `compactionInfo{reason:threshold|overflow|idle|incomplete, action}` + `isCompacting`：`auto_compaction_start/end`。
  - `manualMaintenance`：手动维护（compact/handoff 等）从请求到响应。
  - `switchPending{fromId,toId}`：会话切换窗口。
  - `status`：镜像 sidecar 状态。

### M5 自动重试 / 回退
- 事件序列：`auto_retry_start → (retry_fallback_applied) → auto_retry_end | retry_fallback_succeeded`。
- 门禁：`retryPendingTabs`（per-tab Set）抑制中间 `agent_end` 的通知/错误 toast（`use-rpc-events.ts:64-90`）。
- UI：`TurnStatusRow` 的 retry 行（倒计时/attempt/max/error）与 `ChatStream` 的 `showStatusRow` 条件（`ChatStream.tsx:205-209`）。

### M6 计划审批
- `PlanApprovalSubmitState = {kind:"approve"; option?: execute|compact|keep_context|save} | {kind:"refine"|"dismiss"}`（`renderer/stores/plan-approval.ts:21-22`）。
- store 状态：`pending`（proposal|null）、`feedback`、`notice`、`submitting`（31-32）。
- 迁移：`showProposal`（最新覆盖）→ `setSubmitting` → 成功 `settleTabPlanApproval({clear,exitPlanMode})` / 失败保留 + `notice` / `clearProposal`。

### M7 扩展 UI 请求队列
- 队列：`pendingRequests: ExtensionUIRequest[]`（`renderer/stores/extension-ui.ts:12-16`）；跨 tab 聚合 `useAllPendingRequests`（focused 优先）。
- 方法域：`select | confirm | input | editor | open_url | askDialog | ask | notify | setStatus | setWidget | setTitle | set_editor_text | cancel`。
- 阻塞集合：`BLOCKING_UI_METHODS = {select, confirm, askDialog, ask}`（`shared/rpc-types.ts:1993`）→ 驱动 `useAwaitingConfirmation`。
- 生命周期：push → 渲染最旧 → 用户响应/超时 cancel → `extension_ui_response` → remove。

### M8 更新器
- 状态（`shared/ipc-types.ts:330-344`）：`idle | checking | available{version,notes,mode} | downloading{version,mode,percent,transferred,total} | downloaded{version,mode} | not-available{version} | error{message,showInBanner}`。
- `UpdateInstallMode = "automatic" | "manual"`。
- 渲染层恢复集合 `RECOVERED = ["available","not-available","downloaded"]`（`renderer/stores/updater.ts:20`）用于重新武装被 dismiss 的错误。
- 迁移来源：`main/updater.ts` 的 autoUpdater 事件 + 手动 IPC（212-315）。

### M9 定时任务
- `TaskState = active | paused | disabled | archived`；`TaskDisabledReason = failure_streak | blocked_config`（`shared/scheduler-types.ts:18-19`）。
- `RunState = queued | running | succeeded | failed | cancelled | timed_out | skipped_overlap | skipped_backlog | missed | blocked_config | unknown`（72-83）。
- `RunReview.state = none | pending | accepted | rejected`（102）；`RunVerification.state = none | passed | failed | error`（96）；`RunReport.status = done | nothing_to_report | blocked | partial`（88）。
- `SchedulerHealthState = healthy | paused | stalled`（160）。
- 取消原因：`user | app_quit | task_archived`（139）。

### M10 听写
- `DictationPhase = opening | preparing | downloading | recording | transcribing`（`renderer/lib/dictation.ts:13-22`）。
- 帧驱动：`host_dictation_state{recording|transcribing|idle}`、`host_dictation_download`、`host_dictation_text{volatile|clear|commit|trim|submit}`、`host_dictation_notice`（147-193）。

### M11 子代理
- 线上 `status: string`（自由格式，`shared/rpc-types.ts:1852-1870`）。
- 本地 live 集合：`started | running | pending | idle | parked | stale`（`renderer/stores/subagents.ts:42-50`）。
- 归一：`running && live === false → stale`（52-54）；终态示例 `completed | failed | cancelled | aborted`（DAG/表格着色）。

### M12 终端
- `RpcTerminalInfo.running: boolean` + `exitCode?`（`shared/rpc-types.ts:740-756`）；view 侧 `exited` 标记（`terminals.ts` `view.exited`）。
- 帧：`terminal_opened | terminal_output | terminal_exit{cancelled,timedOut} | terminal_closed`（769-796）。

### M13 浏览器页
- `IpcBrowserPage{openedBy: "agent"|"user", agentDriving: boolean, loadError?}`；`IpcBrowserState{tabId,pages,activePageId,agentLinked}`（`shared/ipc-types.ts:974-1000`）。
- agent 活跃窗口：`activeUntil = now + AGENT_IDLE_MS(30s)`，`idleTimer` 到点触发 `#agentIdleCheck`（`main/browser-host.ts:858-870`）。

### M14 tab 路由（可见选择 vs 已路由）
- 两个变量：`selectedTabId`、`routedTabId` + `routeReady`（`renderer/lib/tab-routing.ts`）。
- 迁移：`beginTabRoute(prev,next)` → ready=false；`settleTabRoute(tabId)` → routed=tabId、ready=(selected===tabId)；`reconcileTabRoute(tabId, settled)`；`resetTabRoute()`。
- 判据：`acceptsActiveTabEvents() = routeReady && (selected === null || routed === null || selected === routed)`。

### M15 队列 lane
- `QueueLane = "steering" | "followUp"`（`renderer/stores/queue.ts:16`）；两 lane 独立排序，跨 lane 通过 `queue_move(toLane)` 追加到目标 lane 末尾（`QueuePanel.tsx:1-9`）。

### M16 主题选择
- `ThemeSelection = "system" | ThemeName | "custom:<id>"`（`renderer/lib/themes.ts:1723-1726`），`THEMES` 为内置 id 表（1696）。
- 应用副作用三分支：inline token（命名主题）、`data-theme`/color-scheme（`lib/theme.ts`）、persist（`applyThemeByName` opts）。

### M17 更新提示 dismiss
- `UpdateDismissal{version?, error?}` 持久化在 localStorage；`RECOVERED` 状态重新武装 error dismiss（`renderer/stores/updater.ts:14-80`）。

## 复刻提示（与 flows 直接相关）

1. **事件合批与 tab 路由是前置条件**：F01–F26 全部依赖 `rpc:events` 批 + `tabId`；`beginTabRoute/settleTabRoute` 的「选中先于路由」窗口必须在 Rust/IPC 层保持同样的可观测性，否则 F05/F07 会出现串台。证据：`use-rpc-events.ts` `reduceEvents`、`tab-routing.ts`。
2. **五个「原 tab 回包」通道**：`extension-ui:respond`、`host-tool:result/update`、`host-uri:result` 必须按 request id 路由回原 tab（`main/ipc.ts:591-620`），这是 F11/F12/F17 的 host-tool 基础。
3. **长超时清单**（否则这些 flow 会假失败）：`get_available_models`/`get_providers`/`login`/`get_settings_schema`（F04/F19）、`compact`/`handoff`（F09）、`switch_session`/`open_session`（F07）、`export_html`、`get_messages_page`。证据：`shared/rpc-client.ts` 的 per-command timeout 包装与各调用点。
4. **stock sidecar 降级点**（影响 F12/F15/F16/F17/F19/F21）：`terminal_*`、`browser_host_*`、`get_jobs`、`get_settings_schema`、`get_available_commands` 的 fork-only 差异——能力探测为假时应隐藏/降级对应 flow，而不是报错。
