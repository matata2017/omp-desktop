# 数据模型 recon — omp-gui v0.9.35

范围：`src/shared/{rpc-types,ipc-types,scheduler-types,stats-types,launch-profile,quotas}.ts`、`src/main/{session-index,models-config,tab-layout}.ts`；落盘证据另引 `src/main/{index,ipc,window,custom-themes,conversation-index,stats-client}.ts` 与 `src/main/scheduler/store.ts`。源码树：本仓库 `src/renderer` + `src/shared`（tag v0.9.35，MIT）与 `D:/tmp/ompgui-src`（主进程源码）。

约定：

- 字段列格式 `name: type`，`?` 表示 TS 可选（`?:`），`/` 在表内代替联合类型的 `|`（避免 Markdown 表格转义噪声）。
- 证据写作 `文件:符号`；`shared/` = `src/shared/`，`main/` = `src/main/`。
- 置信度：`high` = 直接抄自类型/实现；`medium` = 两处代码交叉推出；`low` = 推断，单元格内标 `evidence: 推断`。
- 实体 ID 只是本文件的稳定引用，不来自源码。

---

## 1. 实体表

### 1.1 会话与消息（E01–E08）

| ID | 实体 | fields (name:type) | relationships | evidence (file:symbol) | confidence |
|---|---|---|---|---|---|
| E01 | Session（一个 agent 会话） | 运行态 `RpcSessionState`：`collab?: RpcCollabState`; `model?: ModelInfo / null`; `thinkingLevel: ThinkingLevel / undefined`; `thinkingConfigured?: ThinkingLevel / "auto"`; `availableThinkingLevels?: ThinkingLevel[]`; `isStreaming: boolean`; `isCompacting: boolean`; `steeringMode: "all" / "one-at-a-time"`; `followUpMode: "all" / "one-at-a-time"`; `interruptMode: "immediate" / "wait"`; `sessionFile?: string / null`; `cwd: string`; `sessionId: string`; `sessionName?: string / null`; `fastModeEnabled: boolean`; `fastModeActive: boolean`; `slowModeSupported?: boolean`; `slowModeEnabled?: boolean`; `slowModeScope?: "session" / "global"`; `tokensPerSecond: number / null`; `autoCompactionEnabled: boolean`; `autoRetryEnabled: boolean`; `messageCount: number`; `queuedMessageCount: number`; `hasPendingAsyncWork?: boolean`; `isSettled?: boolean`; `todoPhases: TodoPhase[]`; `systemPrompt?: string[]`; `dumpTools?: ToolDump[]`; `contextUsage?: ContextUsage / null`; `planModeEnabled: boolean`; `prewalkArmed?: boolean`; `agentsPaused: boolean`; `agentsPausedAt?: number`; `kind?: "chat"`。索引视图 `SessionInfo`：`path: string`; `id: string`; `title: string / null`; `cwd: string`; `created: string`; `modified: string`; `messageCount: number`; `size: number`; `status: "complete"/"interrupted"/"aborted"/"error"/"pending"/"unknown"`; `kind?: "chat"`; `parentSessionPath?: string`; `firstMessage: string` | 1—N E02（JSONL 行）；1—N E06（子代理 transcript）；1—1 E09（`tab:get-session-owner` / `IpcSessionOwner`）；fork/branch 由 `parentSessionPath` 与 E05 表达；`session_info_update` 帧携带 `title?`/`sessionId?` | `shared/rpc-types.ts:RpcSessionState`, `shared/rpc-types.ts:SessionInfoUpdateFrame`, `shared/ipc-types.ts:SessionInfo`, `main/session-index.ts:SessionIndex.#doParse` | high |
| E02 | SessionEntry（会话文件中的一行） | 首行 title 槽：`type: "title"`; `v: 1`; `title: string`（另有 agent 侧补齐字段，GUI 不读，evidence: 推断）。第二行 header：`type: "session"`; `version: 3`; `id: string`; `timestamp: ISO string`; `cwd: string`; `parentSession?: string`; `kind?: "chat"`; `title?: string`。消息行：`type: "message"`; `id: string`（= entryId）; `timestamp: ISO string`; `message: {role, content, timestamp?}`。其它行 `type` 取值：`thinking_level_change`、`agent_end`、`turn_end`、`error`、`aborted`、`interrupted`、`message_start`、`tool_execution_start` | N—1 E01；`type:"message"` 行 1—1 E03；`id` 对应 E05 `entryId`；`main/conversation-index.ts` 用同一 `entryId` 做搜索命中定位 | `main/session-index.ts:#doParse`, `#parseFirstMessage`, `#parseTail`; `main/conversation-index.ts:parseMessageLine`; `main/session-index.test.ts`（`{"type":"title","v":1,...}` 与 `{type:"session",version:3,...}` fixture） | high（title 槽补齐字段为 low） |
| E03 | AgentMessage（wire 消息） | `entryId?: string`; `optimistic?: boolean`; `optimisticAfterEntryId?: string / null`; `optimisticDelivered?: boolean`; `role: "user"/"assistant"/"system"/"toolResult"/"bashExecution"/"pythonExecution"/"custom"/"hookMessage"/"branchSummary"/"compactionSummary"/"fileMention"`; `content?: MessageContent[] / string`; `steering?: boolean`; `stopReason?: string`; `errorMessage?: string`; `errorId?: number`; `timestamp?: string / number`; `command?: string`; `code?: string`; `output?: string`; `exitCode?: number`; `cancelled?: boolean`; `truncated?: boolean`; `toolCallId?: string`; `toolName?: string`; `details?: unknown`; `isError?: boolean`; `excludeFromContext?: boolean`; `ttft?: number`; `customType?: string`; `display?: boolean`; `summary?: string`; `shortSummary?: string`; `tokensBefore?: number`; `tokensAfter?: number`; `method?: string`; `files?: Array<{path: string; content: string; lineCount?: number; byteSize?: number; skippedReason?: "tooLarge"/"binary"; image?: ImageContent}>`; `[key: string]: unknown`。`MessageContent = TextContent / ImageContent / ThinkingContent / ToolCallContent`；`TextContent{type:"text", text}`；`ImageContent{type:"image", data, mimeType}`；`ThinkingContent{type:"thinking", thinking}` | 1—1 E02（`entryId` 关联持久行）；1—N E04（`content` 中的 toolCall）；`role:"toolResult"` 回指 E04 `toolCallId` | `shared/rpc-types.ts:AgentMessage`, `MessageContent`, `TextContent`, `ThinkingContent`, `ToolCallContent`; `shared/rpc-types.ts:MessagesPage{messages,totalMessages,nextCursor?}` | high |
| E04 | ToolCall（工具调用） | 内容块 `ToolCallContent`：`type: "toolCall"`; `id: string`; `name: string`; `arguments: Record<string, unknown>`; `partialArgs?: string`; `streamIndex?: number`; `intent?: string`。事件：`tool_execution_start{toolCallId, toolName, args: Record<string,unknown>, intent?}`; `tool_execution_update{toolCallId, toolName, args, partialResult: unknown}`; `tool_execution_end{toolCallId, toolName, result: unknown, isError?}` | N—1 E03；归属于 E01；子代理里由 E06 `AgentProgress.currentTool/currentToolArgs/recentTools` 汇总 | `shared/rpc-types.ts:ToolCallContent`, `AgentSessionEvent`（三个 tool_execution_* 分支） | high |
| E05 | SessionTreeNode（分支树节点） | `entryId: string`; `parentId: string / null`; `role: "user"/"assistant"/"system"`; `textPreview: string`; `timestamp: number`; `label?: string`; `onActiveBranch: boolean`; `isLeaf: boolean`；结果 `RpcSessionTreeResult{tree: RpcSessionTreeNode[]; activeLeafId: string / null}` | N—1 E01；`entryId` ↔ E02 行 `id`；`switch_leaf`/`branch`/`fork_from` 以 `entryId` 定位 | `shared/rpc-types.ts:RpcSessionTreeNode`, `RpcSessionTreeResult` | high |
| E06 | SubagentRun（子代理一次运行） | 快照 `SubagentSnapshot`：`id: string`; `index: number`; `agent: string`; `agentSource?: "bundled"/"user"/"project"`; `description?: string`; `status: string`; `live?: boolean`; `task?: string`; `assignment?: string`; `sessionFile?: string`; `lastUpdate: number`; `spawnedAt?: number`; `progress?: AgentProgress`; `parentToolCallId?: string`; `parentSubagentId?: string`; `kind?: "sub" / "advisor"`。`AgentProgress`：`index`, `id`, `agent`, `agentSource`, `status: "pending"/"running"/"completed"/"failed"/"aborted"`, `task`, `assignment?`, `description?`, `lastIntent?`, `currentTool?`, `currentToolArgs?`, `currentToolStartMs?`, `recentTools: Array<{tool: string; args: string; endMs: number}>`, `recentOutput: string[]`, `toolCount`, `requests`, `tokens`, `contextTokens?`, `contextWindow?`, `cost`, `durationMs`, `modelOverride?: string / string[]`, `modelRole?: string`, `resolvedModel?`, `resolvedModelIsFallback?`, `extractedToolData?: Record<string, unknown[]>`, `retryState?{attempt,maxAttempts,delayMs,errorMessage,startedAtMs}`, `retryFailure?{attempt,errorMessage}`, `inflightTaskDetails?` | N—1 E01（root spawn）；自嵌套：`parentSubagentId` 指向父 E06；`parentToolCallId` → E04；`sessionFile` 指向自己的 E02 文件（artifact 目录） | `shared/rpc-types.ts:SubagentSnapshot`, `AgentProgress`, `SubagentLifecyclePayload`, `SubagentProgressPayload`; `main/session-index.ts:#subagentFiles` | high |
| E07 | TodoPhase（待办阶段） | `name: string`; `tasks: TodoTask[]` | 1—N E08；N—1 E01（`todoPhases`，由 `set_todos` 整表下发） | `shared/rpc-types.ts:TodoPhase` | high |
| E08 | TodoTask（待办项） | `content: string`; `status: "pending" / "in_progress" / "completed" / "abandoned" / "blocked"` | N—1 E07；`todo_reminder` 事件携带 `TodoTask[]` + `attempt`/`maxAttempts` | `shared/rpc-types.ts:TodoTask`, `AgentSessionEvent`（`todo_reminder`） | high |

### 1.2 窗口与标签（E09–E13）

| ID | 实体 | fields (name:type) | relationships | evidence (file:symbol) | confidence |
|---|---|---|---|---|---|
| E09 | Tab（一个 sidecar + 其会话） | `tabId: string`（snowflake）; `cwd: string`; `status: TabStatus`（`SidecarStatus / "running"`，`SidecarStatus = "asleep"/"starting"/"ready"/"exited"/"error"/"restarting"`）; `active?: boolean`; `visible?: boolean`; `group?: {axis: "columns"/"rows"; index: 0/1; ratio: number; partnerTabId: string}`; `compacting?: boolean`; `kind: SessionKind`（`"agent"/"chat"`，spawn 时固定）; `placeholder?: boolean`; `worktree?: IpcTabWorktree{name,branch,baseCwd}`; `sessionPath?: string / null`; `sessionId?: string`; `title?: string / null`; `attention?: "retrying"/"failed"/null`; `attentionDetail?: string`; `retry?: IpcTabRetry / null`。`IpcTabRetry{attempt, maxAttempts, delayMs, errorMessage, startedAt}` | N—1 E12（窗口）；1—1 E01（`sessionPath` 所有权，F-OWN）；N—M E11（split 组）；spawn 载荷 `IpcSpawnTabPayload{cwd?, sessionPath?, kind?, defaultWorkspace?, worktree?}`；结果 `IpcSpawnTabResult{tabId: string/null, cwd?, ownerTabId?, ownerWinId?, refusal?: "owned"/"kind-mismatch"}` | `shared/ipc-types.ts:IpcTabInfo`, `IpcTabRetry`, `TabStatus`, `IpcSpawnTabPayload`, `IpcSpawnTabResult`, `IpcSessionOwner`; `shared/rpc-types.ts:SidecarStatus` | high |
| E10 | TabLayout（一个窗口的持久标签布局） | `version: 1`; `tabs: PersistedTabDescriptor[]`; `activeIndex: number`; `groups?: PersistedTabGroup[]`; `split?: PersistedTabGroup`（gui#30 前的单一分屏，读一次迁入 `groups`）。`PersistedTabDescriptor{cwd: string; sessionPath?: string; kind: SessionKind; worktree?: IpcTabWorktree; placeholder?: boolean; title?: string}`；`MAX_PERSISTED_TITLE = 120` | 1—N E09（描述符恢复成 tab）；1—1 E12（每个窗口一份）；持久化在 E13 `tabLayouts`（数组，窗口顺序）；坏项丢弃规则见 `sanitizePersistedTabLayout` | `main/tab-layout.ts:PersistedTabLayout`, `PersistedTabDescriptor`, `TAB_LAYOUT_VERSION`, `sanitizePersistedTabLayout`; `main/index.ts:MainPrefs.tabLayouts` | high |
| E11 | TabGroup（分屏对） | 持久形态 `PersistedTabGroup`：`axis: "columns"/"rows"`; `firstIndex: number`; `secondIndex: number`; `ratio: number`（读取时 clamp 到 0.2–0.8）。运行态 `IpcTabGroup`：`axis: "columns"/"rows"`; `firstTabId: string`; `secondTabId: string`; `ratio: number` | N—N E09；属于 E10（`groups[]`）或 `IpcSetTabViewPayload` 的运行时入参 | `main/tab-layout.ts:PersistedTabGroup`, `sanitizePersistedTabLayout`（ratio clamp）; `shared/ipc-types.ts:IpcTabGroup`, `IpcSetTabViewPayload` | high |
| E12 | Window（应用窗口） | 运行态 `WindowRecord{win: BrowserWindow; id: number（= webContents.id）; cwd: string; pendingSessionPath?: string}`。持久 `WindowState{x?: number; y?: number; width: number; height: number; isMaximized?: boolean}`；默认 1400×900，最小 800×600。另有 `DeepLinkPayload = {action:"new-session"} / {action:"switch-session"; sessionId}` 与 `MenuActionPayload{approvalMode?, cwd?}` | 1—N E09；1—1 E10；`pendingSessionPath` 由 `sessions:consumePendingOpen` 消费；`IpcSessionOwner{tabId,winId}` 反向指回窗口 | `main/window.ts:WindowRecord`, `WindowState`, `DEFAULT_WIDTH/HEIGHT`, `MIN_WIDTH/HEIGHT`; `shared/ipc-types.ts:IpcSessionOwner`, `DeepLinkPayload` | high |
| E13 | Prefs（GUI 偏好，开放键表） | 类型只有 `PrefsSchema { [key: string]: unknown }` / `MainPrefs { lastProject?: string; proxyUrl?: string; tabLayouts?: PersistedTabLayout[]; tabLayout?: PersistedTabLayout; [key: string]: unknown }`。代码中实际读写过的键：`lastProject`; `proxyUrl`; `tabLayouts`; `tabLayout`(legacy); `launchProfiles`（`Record<cwd, LaunchProfile>`）; `windowState`（独立 store `window-state`）; `language`; `themeName`; `theme`(legacy); `sidebarWidth`; `keymapOverrides`; `approvalMode`(legacy，迁移后置 null); `sidebarQuotas`; `defaultPanelTab`; `tabWidth`; `tabRows`; `notifications`; `notificationsWhenFocused`; `thinkingExpanded`; `transcriptDetail`; `fontSize`; `codeLineNumbers`; `inputHistory`; `sidebar`; `skillReview`; `displayPreferences.<key>` | N—1 应用（全局单例）；1—N E10；1—N E14（`launchProfiles.<cwd>`）；1—1 E12 的 `windowState`；E19 主题选择的落点（`themeName`/`theme`） | `main/ipc.ts:PrefsSchema`, `prefsStore`; `main/index.ts:MainPrefs`; `main/window.ts:StoreSchema`; `renderer/lib/display-preferences.ts:saveGuiPreference`; `renderer/lib/themes.ts`; `renderer/stores/{input-history,sidebar-prefs,skill-review}.ts:PREFS_KEY` | high（键清单为代码出现过的集合，非封闭 schema） |

### 1.3 配置、模型与主题（E14–E21）

| ID | 实体 | fields (name:type) | relationships | evidence (file:symbol) | confidence |
|---|---|---|---|---|---|
| E14 | LaunchProfile（按工作区的 CLI 启动参数） | `systemPrompt?: string`（`--system-prompt`）; `appendSystemPrompt?: string`（`--append-system-prompt`）; `noRules?: boolean`（`--no-rules`）; `addDirs?: string[]`（`--add-dir` 重复）; `tools?: string[]`（`--tools` csv）; `noLsp?: boolean`（`--no-lsp`）; `planYolo?: boolean`（`--plan-yolo`）; `profile?: string`（`--profile`）; `sessionDir?: string`（`--session-dir`）; `config?: string`（`--config`）。常量：`DENYLISTED_FLAGS` 15 项；`DENYLISTED_WITH_VALUE` 7 项；`VALUED_FLAGS` 7 项 | N—1 E13（`launchProfiles.<cwd>`）；输出 flags 追加到 E09 的 sidecar argv（`profileToFlags` → `stripDenylistedFlags`） | `shared/launch-profile.ts:LaunchProfile`, `DENYLISTED_FLAGS`, `DENYLISTED_WITH_VALUE`, `VALUED_FLAGS`, `profileToFlags`, `parseLaunchProfile`, `stripDenylistedFlags`; `main/sidecar.ts:#spawn`（`loadLaunchProfileFlags`） | high |
| E15 | ModelRole（角色模型绑定） | `ModelRoleEntry`：`id: string`; `name: string`; `tag?: string`; `color?: string`; `hidden?: boolean`; `section: "chat" / "kind"`; `model?: string`; `selected?: ModelRoleSelection{provider, id, thinkingLevel?: ThinkingLevel/"auto"}`; `source: string`; `candidates: ModelRoleCandidate[]`。`ModelRoleCandidate{provider, id, name, kind: "chat"/"tiny"/"image"/"tts"/"stt"/"search"/"judge", thinkingLevels?: (ThinkingLevel/"auto")[]}`。`set_model_role` 入参：`role: string`; `modelId: string / null`; `thinkingLevel?: ThinkingLevel / "auto"` | N—1 E01（生效于活动会话）；候选来自 E16；`get_model_role_metadata` 只回 `ModelRoleMetadata{id,name,tag?,color?,hidden?,section}` | `shared/rpc-types.ts:ModelRoleEntry`, `ModelRoleCandidate`, `ModelRoleSelection`, `ModelRoleMetadata`, `ModelRolesResult`; `RpcCommand`（`set_model_role`） | high |
| E16 | AvailableModel（可选模型目录项） | `ModelInfo`：`provider: string`; `id: string`; `name?: string`; `description?: string`; `isNew?: boolean`; `isBeta?: boolean`; `isRecommended?: boolean`; `int?: number / null`; `tps?: number / null`; `contextWindow?: number / null`; `maxTokens?: number / null`; `cost?: ModelCost{input?, output?, cacheRead?, cacheWrite?}`; `api?: string`; `input?: string[]`; `compat?: object`。目录结果 `AvailableModelsResult{models: ModelInfo[]; discoveryStates: ProviderDiscoveryState[]; refreshPending: boolean; generation: number}`；`ProviderDiscoveryState{provider, status: "idle"/"ok"/"empty"/"cached"/"unavailable"/"unauthenticated", optional, stale, fetchedAt?, models: string[], error?}`；推送帧 `ModelCatalogUpdateFrame extends AvailableModelsResult {type:"model_catalog_update"; providers: ProviderInfo[]}` | N—1 E17；被 E15 的 `candidates` 引用；`set_model{provider, modelId}` 选中后写入 E01 `model` | `shared/rpc-types.ts:ModelInfo`, `ModelCost`, `AvailableModelsResult`, `ProviderDiscoveryState`, `ModelCatalogUpdateFrame` | high |
| E17 | Provider（模型服务商） | 目录态 `ProviderInfo`：`id: string`; `name: string`; `authenticated: boolean`; `authKind?: "oauth"/"apikey"/"env"`; `account?: string`; `loginAvailable: boolean`; `disabled: boolean`; `baseUrl?: string`; `modelCount: number`。GUI 可编辑（models.yml）形态 `CustomProviderInput`：`id`; `api: CustomProviderApi`; `baseUrl: string`; `apiKey?: string`; `clearApiKey?: boolean`; `auth?: "apiKey"/"none"/"oauth"`; `authHeader?: boolean`; `headers?: Record<string,string>`; `discovery?: CustomProviderDiscovery{type: "ollama"/"llama.cpp"/"lm-studio"/"openai-models-list"/"proxy"/"litellm"; timeoutMs?}`; `disableStrictTools?: boolean`; `transport?: "pi-native"`; `extraBody?: Record<string, unknown>`; `models: CustomProviderModelInput[]`。模型项 `CustomProviderModelInput{id, name?, api?, baseUrl?, reasoning?, thinking?{mode, efforts[], defaultLevel?, supportsDisplay?}, input?: ("text"/"image")[], supportsTools?, cost?{input?,output?,cacheRead?,cacheWrite?}, premiumMultiplier?, contextWindow?, maxTokens?, omitMaxOutputTokens?, headers?}`；视图 `CustomProviderView` 额外 `hasApiKey: boolean`; `apiKeyPreview?: string`; `builtin: boolean` | 1—N E16；1—N E18（登录账户）；`models.yml` 中 `providers.<id>` 即本实体；内置 id 集 `BUILTIN_PROVIDERS`（88 项，只读，不可新建/删除） | `shared/rpc-types.ts:ProviderInfo`, `ProvidersResult`; `shared/ipc-types.ts:CustomProviderInput`, `CustomProviderModelInput`, `CustomProviderView`, `CUSTOM_PROVIDER_APIS`, `CUSTOM_MODEL_EFFORTS`; `main/models-config.ts:modelsPath`, `toView`, `upsertModelsProvider` | high |
| E18 | LoginAccount（登录账户 / 凭据） | 登录入口 `LoginProvider{id, name, available, authenticated}`；登出选择 `LogoutAccount{credentialId: number; provider: string; label: string; detail: string; type: "api_key"/"oauth"; active: boolean}`；用量 `UsageReport{provider: string; fetchedAt: number; limits: UsageLimit[]; notes?: string[]; account?: string; resetCreditsAvailable?: number}`，`UsageLimit{id,label,usedFraction?,used?,limit?,unit?,remainingFraction?,windowLabel?,resetsAt?,status?,notes?}`；额度重置 `RpcResetCreditAccount{label, provider, providerLabel, availableCount, redeemableCount, target: RpcResetCreditTarget{provider, credentialId, creditId?, accountId?, email?, orgId?}, active, error?, unavailableReason?, expiresAt?}` | N—1 E17；`logout{providerId, credentialId}` 删除；`redeem_reset_credit{target,label}` 消耗 | `shared/rpc-types.ts:LoginProvider`, `LogoutAccount`, `UsageReport`, `UsageLimit`, `RpcResetCreditTarget`, `RpcResetCreditAccount` | high |
| E19 | Theme（UI 主题） | 可选项 `RpcThemeInfo{name: string; path?: string}`；解析色 `RpcThemeColorsResult{name: string; colors: Record<string, string>}`（键为 theme token，如 accent/statusLineBg）；插件贡献 `RpcGuiThemeInfo{id: string; tokens: Record<string, string>}`；自定义文件 `IpcCustomThemeFile{id: string; fileName: string; data?: unknown; error?: string}`，集合 `IpcCustomThemes{dir: string; files: IpcCustomThemeFile[]}`。选择值：内置名 或 `"custom:<id>"`；`"system"` 为特殊值 | 自定义主题来自 E13 之外的文件（`~/.omp/gui/themes/*.json`）；选择持久化在 E13 `themeName`（+ legacy `theme`）；插件主题来自插件生态 | `shared/rpc-types.ts:RpcThemeInfo`, `RpcThemeColorsResult`, `RpcGuiThemeInfo`; `shared/ipc-types.ts:IpcCustomThemeFile`, `IpcCustomThemes`; `main/custom-themes.ts:customThemesDir`, `readCustomThemes`; `renderer/lib/themes.ts`（`themeName`/legacy `theme`） | high |
| E20 | Skill（可发现技能） | `RpcSkillInfo`：`name: string`; `description: string`; `source: string`（`"<provider>:<level>"`）; `enabled: boolean`; `location: string`; `provider: string`; `providerName: string`; `level: "user"/"project"/"native"`; `managed: boolean`; `hidden: boolean`; `modifiedAt?: number`；详情 `RpcSkillDetail extends RpcSkillInfo {body: string}`；写操作 `manage_skill{action:"create"/"update"/"delete", name, description?, body?}` | N—1 E01（会话级 enable 状态）；文件位于 user/project/native 三级（`location`）；`get_skill_detail{name}` 取 body | `shared/rpc-types.ts:RpcSkillInfo`, `RpcSkillsResult`, `RpcSkillDetail`, `RpcManageSkillResult`, `RpcCommand`（`set_skill_enabled`,`manage_skill`） | high |
| E21 | McpServer（MCP 服务） | `RpcMcpServerInfo`：`name: string`; `transport: "stdio"/"http"/"sse"/"unknown"`; `status: "connected"/"connecting"/"disconnected"`; `toolCount: number`; `enabled: boolean`; `authed: boolean`; `scope?: "user"/"project"`; `command?: string`; `url?: string`; `lastError?: string`; `authState?: "none"/"authorized"/"expired"/"required"`。输入 `RpcMcpServerInput{transport: "stdio"/"http"/"sse"; command?; args?: string[]; env?: Record<string,string>; url?; headers?: Record<string,string>; timeoutMs?}` | N—1 E01（连接挂在该会话的 sidecar 上）；`mcp_action{action:"enable"/"disable"/"reconnect"/"remove", scope?}` 变更；工具以 `source:"mcp"` 进入 `get_active_tools` | `shared/rpc-types.ts:RpcMcpServerInfo`, `RpcMcpServersResult`, `RpcMcpServerInput`, `RpcActiveTool` | high |

### 1.4 扩展协议（E22–E24）

| ID | 实体 | fields (name:type) | relationships | evidence (file:symbol) | confidence |
|---|---|---|---|---|---|
| E22 | ExtensionUIRequest（sidecar→GUI 的阻塞式 UI 请求） | 判别式 `method`：`select{id,title,options: string[],optionDetails?: {description?}[],timeout?}`; `confirm{id,title,message,timeout?}`; `askDialog{id,questions: ExtensionAskDialogQuestion[],timeout?}`; `ask{id,questions,timeout?}`; `input{id,title,placeholder?,timeout?,authCodeFor?}`; `editor{id,title,prefill?,promptStyle?}`; `notify{id,message,notifyType?: "info"/"warning"/"error"}`; `setStatus{id,statusKey,statusText?}`; `setWidget{id,widgetKey,widgetLines?: string[],widgetPlacement?: string}`; `setTitle{id,title}`; `set_editor_text{id,text,images?,prepend?}`; `open_url{id,url,launchUrl?,instructions?}`; `cancel{id,targetId}`（共 13 种）。响应 `ExtensionUIResponse` 5 变体：`value` / `confirmed` / `askDialog` / `answers[{id,selectedOptions,customInput?}]` / `cancelled{timedOut?}`。问题项 `ExtensionAskDialogQuestion{id,question,header?,options: ExtensionAskDialogOption[],...}` | 1—1 E09（`IpcExtensionUiPayload{tabId, request}` 路由回原 tab，F-UI-ORIGIN）；`timeout` 与 `BLOCKING_UI_METHODS`（select/confirm/askDialog/ask/input/editor）决定渲染层等待态 | `shared/rpc-types.ts:ExtensionUIRequest`, `ExtensionUIResponse`, `ExtensionAskDialog*`, `BLOCKING_UI_METHODS`; `shared/ipc-types.ts:IpcExtensionUiPayload`, `IpcExtensionUiRespondPayload` | high |
| E23 | HostToolCall（GUI 提供给 agent 的工具调用） | 定义 `HostToolDefinition{name, label?, description, parameters: Record<string,unknown>, hidden?, loadMode?: "essential"/"discoverable"}`；调用 `HostToolCallRequest{type:"host_tool_call"; id: string; toolCallId: string; toolName: string; arguments: Record<string,unknown>}`；结果 `HostToolResult{type:"host_tool_result"; id; result: HostToolOutput{content: {type:"text"; text: string}[]; details?: Record<string,unknown>}; isError?}`；流式 `HostToolUpdate{type:"host_tool_update"; id; partialResult: HostToolOutput}`；取消 `HostToolCancelRequest{type:"host_tool_cancel"; id; targetId}`。同族 URI 协议：`HostUriRequest{type:"host_uri_request"; id; operation:"read"/"write"; url; content?}` → `HostUriResult{type:"host_uri_result"; id; content?; error?}` | 1—N E04（`toolCallId` 回填到工具卡）；N—1 E09（请求按 id 记录发起 tab）；`set_host_tools{tools}` / `set_host_uri_schemes{schemes}` 注册 | `shared/rpc-types.ts:HostToolDefinition`, `HostToolCallRequest`, `HostToolResult`, `HostToolUpdate`, `HostToolCancelRequest`, `HostUriSchemeDefinition`, `HostUriRequest`, `HostUriResult`; `shared/ipc-types.ts:IpcHostToolCallPayload` | high |
| E24 | Plan（计划文件与批准） | 模式态 `PlanModeState{enabled: boolean; planFilePath?: string}`；提案事件 `plan_proposal{planFilePath: string; title?: string; suggestedFileName?: string; planContent: string; options: string[]}`；读文件载荷 `IpcFsReadPlanPayload{tabId?: string; fsPath: string; localRoot: string / null}`；批准命令 `plan_approval{approved: boolean; option?: "execute"/"compact"/"keep_context"/"save"; feedback?: string; savePath?: string}` | N—1 E01；文件落 `<sessions>/<project>/<session-stem>/…`（会话 artifact 目录）或 `<cwd>/**/*plan.md`（后备取最新）；渲染层以 `local://<slug>-plan.md` 提及 | `shared/rpc-types.ts:PlanModeState`, `AgentSessionEvent`（`plan_proposal`）, `RpcCommand`（`plan_approval`）; `shared/ipc-types.ts:IpcFsReadPlanPayload`; `main/ipc.ts:newestPlanFile`; `renderer/components/layout/input-area-utils.ts:MENTION_SCHEMES` | high（`localRoot` 具体目录形态为 medium） |

### 1.5 计划任务（E25–E27）

| ID | 实体 | fields (name:type) | relationships | evidence (file:symbol) | confidence |
|---|---|---|---|---|---|
| E25 | ScheduledTask（定时任务） | `id: string`; `revision: number`; `state: "active"/"paused"/"disabled"/"archived"`; `disabledReason?: "failure_streak"/"blocked_config"`; `origin: TaskOrigin{sessionPath: string/null; createdBy: "user"/"agent"}`; `createdAt: number`; `updatedAt: number`; `nextFireAt: number / null`；配置 `TaskConfig`：`title`, `prompt`, `cwd`, `schedule: TaskSchedule`（`{kind:"once"; at}` / `{kind:"interval"; everyMs; anchor}` / `{kind:"cron"; expr; tz}`）, `model: string / null`, `tools: "read_only"/"workspace"/"full"`, `depth: TaskDepth`（`{kind:"check"}` / `{kind:"goal"}` / `{kind:"research"; rounds}`）, `timeoutMs: number`, `verify: TaskVerify / null`（`{command, timeoutMs}`）, `notify: "always"/"on_change"/"on_failure"`；草稿 `TaskDraft extends TaskConfig {origin, startPaused}` | 1—N E26；1—N E27；`TaskNotice{id, target: SchedulerOpenTarget{view?, taskId?, runId?}, title, body, tone: "done"/"review"/"failed", createdAt, read}` 挂在任务/运行上 | `shared/scheduler-types.ts:ScheduledTask`, `TaskConfig`, `TaskSchedule`, `TaskDepth`, `TaskOrigin`, `TaskVerify`, `TaskNotice`, `SchedulerOpenTarget`; `main/scheduler/store.ts:StoredTask` | high |
| E26 | TaskRun（一次运行） | `id: string`; `taskId: string`; `taskRevision: number`; `trigger: "schedule"/"manual"/"retry"`; `scheduledFor: number / null`; `createdAt: number`; `state: "queued"/"running"/"succeeded"/"failed"/"cancelled"/"timed_out"/"skipped_overlap"/"skipped_backlog"/"missed"/"blocked_config"/"unknown"`; `cancelReason?: "user"/"app_quit"/"task_archived"`; `startedAt?`; `endedAt?`; `sessionPath?`; `worktree?: RunWorktree{path,branch,baseCwd,removed,committed}`; `report?: RunReport{status: "done"/"nothing_to_report"/"blocked"/"partial"; summary; findings[]; changes[]; next?}`; `blocked: {tool, target?}[]`; `changedFiles: string[]`; `verification: RunVerification{state: "none"/"passed"/"failed"/"error"; exitCode?; outputTail?}`; `review: RunReview{state: "none"/"pending"/"accepted"/"rejected"; note?; at?}`; `usage?: RunUsage{inputTokens,outputTokens,costUsd,turns}`; `model?: string`; `rounds?: number`; `error?: {message, kind: "transient"/"auth"/"config"/"other"}`; `retryOf?: string`; `progress?: RunProgress{turns, lastTool: string/null, round}` | N—1 E25；N—1 E27（`taskRevision`）；`sessionPath` 指向 E01 的一次性会话（`runSessionPaths()` 用于把它们挡出侧栏）；`retryOf` 自指 | `shared/scheduler-types.ts:TaskRun`, `RunReport`, `RunVerification`, `RunReview`, `RunUsage`, `RunWorktree`, `RunProgress`, `RunState`, `RunTrigger`; `main/scheduler/store.ts:saveRun`, `runSessionPaths` | high |
| E27 | TaskRevision（冻结的任务配置版本） | `revision: number`; `createdAt: number`; `config: TaskConfig` | N—1 E25（`(task_id, revision)` 主键）；被 E26 的 `taskRevision` 引用；`configOf(task)` 落盘 | `shared/scheduler-types.ts:TaskRevision`; `main/scheduler/store.ts:saveTask`, `getRevision`, `listRevisions`, `configOf` | high |

### 1.6 统计（E28）

| ID | 实体 | fields (name:type) | relationships | evidence (file:symbol) | confidence |
|---|---|---|---|---|---|
| E28 | StatsRollup（用量汇总行） | 总览 `OverviewData{overall: {totalRequests, successfulRequests, failedRequests, errorRate, totalInputTokens, totalOutputTokens, totalCacheReadTokens, totalCacheWriteTokens, cacheRate, totalCost, avgDuration: number/null, avgTtft: number/null, avgTokensPerSecond: number/null}; byAgentType: AgentTypeRow[]; timeSeries: TimePoint[]}`；`TimePoint{timestamp, requests, errors, tokens, cost}`；模型 `ModelRow{model, provider, totalRequests, failedRequests, errorRate, totalInputTokens, totalOutputTokens, totalCacheReadTokens, totalCacheWriteTokens, cacheRate, totalCost, avgDuration, avgTtft, avgTokensPerSecond}`；工具 `ToolRow{tool, calls, errors, argsChars, resultChars, totalTokensShare, outputTokensShare, costShare, lastUsed}`；文件夹 `FolderRow{folder, totalRequests, failedRequests, errorRate, totalInputTokens, totalOutputTokens, totalCacheReadTokens, totalCacheWriteTokens, totalCost, avgDuration, avgTokensPerSecond}`；请求 `RequestRow{id?, entryId, sessionFile, folder, model, provider, api, timestamp, duration, ttft, stopReason, errorMessage, usage{input,output,cacheRead,cacheWrite,totalTokens,cost{total}}}`；窗口用量 `WindowInsight{provider, windowKey, windowLabel, accounts, cycles, fractionConsumed, estTokensPerWindow, peakConcurrentFraction, idealAccounts, exhaustedEvents}`；增益 `GainData{overall: SourceTotals, bySource: Record<string, SourceTotals>, timeSeries: GainPoint[], project, projects}` | 行由 HTTP API 返回，非 GUI 本地实体；与 E01 的 `SessionStats`/`UsageResult` 相关但不同源（本地 journal vs stats 服务） | `shared/stats-types.ts:OverviewData`, `ModelRow`, `ToolRow`, `FolderRow`, `RequestRow`, `WindowInsight`, `GainData`; `main/stats-client.ts:VALID_PATHS`；`stats.db` 本体证据: 推断（GUI 源码不打开该库） | high（wire 形状）/ low（库表） |

---

## 2. 持久化

### 2.1 存储位置总表

| 存储 | 实体 | 格式 | 写入方式 | 证据 | confidence |
|---|---|---|---|---|---|
| `<agentDir>/sessions/<project>/<name>.jsonl` | E01, E02, E03, E04, E05, E06（子代理 transcript） | NDJSON（UTF-8，一行一 JSON；首行定长 title 槽） | agent（sidecar）独占写入；GUI 只读 + 删除/改名 | `main/session-index.ts:#scanDir`, `#doParse`; `main/agent-paths.ts:agentDir` | high |
| `<agentDir>/models.yml`（存在则优先；否则 `models.yaml`；都不存在时新建 `.yml`） | E17（含内嵌 models、cost、thinking） | YAML（`yaml` 包的 `Document`，保序） | GUI 原子替换：写 `<file>.tmp-<pid>-<n>` 后 `rename`；未渲染字段原样保留 | `main/models-config.ts:modelsPath`, `readModelsFile`, `writeModelsFile`, `upsertModelsProvider` | high |
| `~/.omp/gui/themes/*.json` | E19（自定义主题） | 每个文件一个 JSON token map；≤100 个文件，单文件 ≤256 KiB | 用户/GUI 外部写入；main 只读并解析，250 ms 去抖后广播 | `main/custom-themes.ts:customThemesDir`（`agentDir()/../gui/themes`）, `readCustomThemes`, `CustomThemesWatcher` | high |
| `<userData>/prefs.json` | E13, E10（`tabLayouts`）, E14（`launchProfiles.<cwd>`）, E19 选择 | 单个扁平 JSON 对象（electron-store `name: "prefs"`） | main/renderer 通过 `prefs:set` 按 key 合并写；electron-store 自行落盘 | `main/ipc.ts:prefsStore`（`new Store<PrefsSchema>({name:"prefs"})`）; `main/index.ts:prefsStore`; e2e 直接写 `<userData>/prefs.json` | high |
| `<userData>/window-state.json` | E12（窗口几何） | JSON：`{ "windowState": {x?, y?, width, height, isMaximized?} }` | 窗口 move/resize/close 时 `store.set("windowState", …)` | `main/window.ts:StoreSchema`, `new Store({name:"window-state"})`, 持久化调用 | high |
| `<userData>/scheduler.db` | E25, E26, E27（+ notices/limits/paused/heartbeat 元数据） | SQLite（WAL），见 2.4 | `SchedulerStore` 同步 API（`node:sqlite`） | `main/index.ts`（`join(app.getPath("userData"), "scheduler.db")`）; `main/scheduler/store.ts:31-55` | high |
| `<userData>/conversation-index.db` | 派生搜索索引（E02/E03 的投影，可删） | SQLite（WAL）+ FTS5 `tokenize='trigram'`，见 2.5 | 增量索引（append-only 读偏移） | `main/index.ts:openConversationIndex`; `main/conversation-index.ts:207-228` | high |
| `<userData>/logs/gui-runtime.jsonl` | 运行日志（非实体） | JSONL | 崩溃/运行时报告追加 | `main/runtime-log.ts:runtimeLogPath` | high |
| `~/.omp/stats.db` | E28 | SQLite（由 `omp stats` 服务端拥有；GUI 只经 HTTP 读） | 无 GUI 写入 | `shared/stats-types.ts:1-7`（说明 API 来自 `packages/stats/src/server.ts`）；库路径与表结构 **evidence: 推断**（GUI 源码无引用；plan 记载 `messages/tool_calls/user_messages/message_rollup/tool_rollup/session_rollup/file_offsets/meta`） | low |

`<userData>`：Electron `app.getPath("userData")`，应用名由 `app.setName("omp")` 固定（打包产物 `productName: omp`），Windows 上等于 `%APPDATA%\omp`（目录名由 Electron 行为决定，evidence: 推断/medium）；`--user-data-dir=<path>` 可在启动时覆盖（`main/index.ts:41-46`）。`<agentDir>` = `PI_CODING_AGENT_DIR` 或 `~/.omp/agent`（`main/agent-paths.ts`）。

### 2.2 会话 JSONL（E02）

物理布局：

- 一层项目目录 + 会话文件：`<agentDir>/sessions/<projectDir>/<file>.jsonl`（`#scanDir` 只 `readdir` 一层，`depth: 1` 监听）。
- 子代理 transcript：`<sessionFile 去掉 .jsonl>/<agentId>.jsonl`，可再嵌套，深度 ≤ 8；忽略 `__advisor*` 与名字含 `.bak` 的文件，且非 `.jsonl` 一律不认（`#subagentFiles`）。
- 文件被 GUI 隐藏的条件：`messageCount === 0 && !title && size <= 32 KiB`（且 tail 读覆盖整个文件时）。
- 列表排序：`modified` 倒序；按 `id` 去重（迁移期同一 id 可能在两个项目目录）；`scope:"local"` 只留 `resolve(cwd) === 本窗口 cwd`。

逐行形态（GUI 只依赖这些字段）：

```jsonl
{"type":"title","v":1,"title":""}                                  // 首行，定长 256 B 槽；title 非空则作为会话名
{"type":"session","version":3,"id":"<sessionId>","timestamp":"<ISO>","cwd":"D:\\proj","parentSession":"<path>","kind":"chat"}
{"type":"message","id":"<entryId>","timestamp":"<ISO>","message":{"role":"user","content":"hi"}}
{"type":"message","id":"<entryId>","timestamp":"<ISO>","message":{"role":"assistant","content":[{"type":"text","text":"…"}]}}
{"type":"thinking_level_change","thinkingLevel":"off"}
{"type":"agent_end"}   // 或 turn_end / error / aborted / interrupted / message_start / tool_execution_start
```

解析常量（`main/session-index.ts`）：`TITLE_SLOT_BYTES = 256`；`HEAD_BYTES = 32 * 1024`；`TAIL_BYTES = 32 * 1024`；`MAX_HEADER_BYTES = 1024 * 1024`；`MAX_PARSE_CACHE_ENTRIES = 4096`；`SEARCH_READ_BYTES = 8 MiB`；`SEARCH_CACHE_BYTES = 64 MiB`；`SEARCH_BATCH = 16`。缓存键 `"<mtimeMs>:<size>"`。

推导规则（复刻时必须一致）：

- `title` = title 槽的 `title`，空则 header 的 `title`，再空为 `null`。
- header 是**第二行**；head 32 KiB 内没读到换行时按 `MAX_HEADER_BYTES` 重读到换行（导入会话的元数据可达数 KB）。
- `status` 只看 tail 最后一行的 `type`：`agent_end`/`turn_end`→`complete`；`error`→`error`；`aborted`→`aborted`；`interrupted`→`interrupted`；`message_start`/`tool_execution_start`→`pending`；`message` 且 `message.role==="assistant"`→`complete`，否则`pending`；无行→`unknown`。
- `messageCount` 只统计**尾部 32 KiB 内**的 `type:"message"` 行（不是全文件总数）。
- `firstMessage` 取 head 内第一条 `type:"message"` 且 `role:"user"` 的文本，截 200 字符。
- 删除校验：`resolve(path)` 必须以 `<sessionsDir>/` 开头且以 `.jsonl` 结尾，否则 `Invalid session path`。

### 2.3 `prefs.json`（E13）

扁平 JSON 对象，schema 开放（`[key: string]: unknown`）。结构示意：

```json
{
  "lastProject": "D:\\rust-omp",
  "proxyUrl": "http://127.0.0.1:7897",
  "tabLayouts": [ { "version": 1, "tabs": [ { "cwd": "D:\\p", "kind": "agent", "sessionPath": "…jsonl", "title": "…" } ], "activeIndex": 0, "groups": [ { "axis": "columns", "firstIndex": 0, "secondIndex": 1, "ratio": 0.5 } ] } ],
  "tabLayout": { "version": 1, "tabs": [], "activeIndex": 0 },
  "launchProfiles": { "D:\\p": { "noRules": true, "tools": ["read", "grep"] } },
  "language": "zh",
  "themeName": "custom:midnight",
  "theme": "dark",
  "sidebarWidth": 260,
  "keymapOverrides": { "tab.new": "mod+t" },
  "displayPreferences.showTokenUsage": false,
  "inputHistory": [],
  "sidebar": { "pinnedGroups": [], "pinnedSessions": [] },
  "skillReview": {}
}
```

读侧 sanitize（不可省）：

- `sanitizePersistedTabLayout`：`version !== 1` 或 `tabs` 非数组 → 整份丢弃；`cwd` 不存在、`kind` 非 `agent|chat` 的项丢弃；`sessionPath` 文件不存在则降级为同 cwd 的新 tab；同一 `sessionPath` 只保留首个；`placeholder === true` 的项不恢复（旧版启动 chat 例外判定）；`title` 截 120 字符；`ratio` clamp 到 `[0.2, 0.8]`；一对 tab 只能出现在一个 group。
- `parseLaunchProfile`：非对象→`{}`；未知键丢弃；空白字符串归一为缺失；`addDirs`/`tools` 去空。
- `stripDenylistedFlags`：成对剥离（`--flag value` 与 `--flag=value` 两种），且非 denylisted 的值型 flag 的值视为不透明数据（即使长得像 `--session` 也保留）。

### 2.4 `scheduler.db`（E25–E27）

DDL（`main/scheduler/store.ts:31-55`，`SCHEMA_VERSION = "1"`，`PRAGMA journal_mode=WAL; synchronous=NORMAL`）：

```sql
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS tasks (
  id TEXT PRIMARY KEY,
  state TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  data TEXT NOT NULL            -- 整个 StoredTask 的 JSON（= ScheduledTask 去掉 nextFireAt）
);
CREATE TABLE IF NOT EXISTS task_revisions (
  task_id TEXT NOT NULL,
  revision INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  data TEXT NOT NULL,           -- TaskConfig 的 JSON
  PRIMARY KEY (task_id, revision)
);
CREATE TABLE IF NOT EXISTS runs (
  id TEXT PRIMARY KEY,
  task_id TEXT NOT NULL,
  state TEXT NOT NULL,
  review_state TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  data TEXT NOT NULL            -- TaskRun 的 JSON；progress 不落库（saveRun 时置 undefined）
);
CREATE INDEX IF NOT EXISTS runs_by_task ON runs (task_id, created_at DESC);
CREATE INDEX IF NOT EXISTS runs_by_time ON runs (created_at DESC);
```

`meta` 键（值均为字符串）：`schema`（`"1"`）、`notices`（`TaskNotice[]` 的 JSON）、`limits`（`Partial<SchedulerLimits>` 的 JSON）、`paused`（`"0"/"1"`）、`heartbeat`（毫秒时间戳字符串）。旧记录用 `withDefaults()` 补 `depth`（缺省 `{kind:"goal"}`，`storedDepth` 兼容 `research.rounds` 的 clamp 2–6）。

### 2.5 `conversation-index.db`（派生，可删）

```sql
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS files (
  path TEXT PRIMARY KEY, signature TEXT NOT NULL, indexed_bytes INTEGER NOT NULL,
  tail TEXT NOT NULL, session_id TEXT, cwd TEXT, title TEXT, kind TEXT, modified INTEGER NOT NULL
);
CREATE VIRTUAL TABLE IF NOT EXISTS entries USING fts5(
  body, path UNINDEXED, entry_id UNINDEXED, role UNINDEXED, ts UNINDEXED,
  tokenize = 'trigram'
);
```

`signature` 同样是 `"<mtimeMs>:<size>"`；`indexed_bytes` + 前 64 B（`TAIL_CHECK_BYTES`）`tail` 十六进制用于校验 append-only 读；正文最长 20 000 字符；搜索每次最多 8 个词、400 行、每会话 3 命中、最多 60 会话。

### 2.6 `models.yml`（E17）

```yaml
providers:
  <providerId>:
    api: openai-completions          # CUSTOM_PROVIDER_APIS 之一
    baseUrl: https://api.example.com
    apiKey: sk-…                     # GUI 读回时只给 •••<末4位>
    auth: apiKey                     # apiKey | none | oauth
    authHeader: true
    headers: { X-Trace: "1" }
    discovery: { type: ollama, timeoutMs: 3000 }
    disableStrictTools: true
    transport: pi-native
    compat: { extraBody: { … }, …preserved-keys }
    models:
      - id: my-model
        name: My Model
        api: openai-completions
        baseUrl: https://…
        reasoning: true
        thinking: { mode: effort, efforts: [minimal, low, medium, high], defaultLevel: medium, supportsDisplay: true }
        input: [text, image]
        supportsTools: true
        cost: { input: 1, output: 2, cacheRead: 0.1, cacheWrite: 0.2 }
        premiumMultiplier: 1
        contextWindow: 200000
        maxTokens: 8192
        omitMaxOutputTokens: false
        headers: { … }
```

- 顶层只读写 `providers.<id>`；内置 id（`BUILTIN_PROVIDERS`，88 个字符串常量，条目位于 `main/models-config.ts:50-137`）禁止新建/删除，只允许覆盖已有条目。
- 未渲染字段必须原样保留：`compat`（除 `extraBody`）、`remoteCompaction`、`modelOverrides`、`effortMap`、`contextPromotionTarget`、`compactionModel`；`thinking` 的 `levels`/`minLevel`/`maxLevel` 是同一阶梯的第二来源，保存时删除以免陈旧。
- 写入必须原子（tmp + rename），因为 agent 监听该文件并热加载。

### 2.7 自定义主题（E19）

`~/.omp/gui/themes/<name>.json`，内容为任意 token→颜色字符串的 JSON；渲染层校验每个颜色，缺的 token 由基础主题补齐。id 规则：文件名去 `.json`；为空或与已有 id 重名时用完整文件名。读取上限 100 个文件、单文件 256 KiB；目录变更 250 ms 去抖后整批广播（由 `CustomThemesWatcher.onChange` 驱动）。

---

## 3. RPC 命令面（`RpcCommand`）

来源：`src/shared/rpc-types.ts:21-311`，单一判别联合，**共 197 条命令**。每条都带 `id?: string`（Rust 侧注入 `id` 做 oneshot 关联），命令名即 `type` 字面量。

### 3.1 按前缀统计（前缀 = 第一个 `_` 之前的分词）

61 个不同前缀；`get_` 与 `set_` 合计 93 条（47.2%）。

| 前缀 | 数量 | 前缀 | 数量 | 前缀 | 数量 |
|---|---:|---|---:|---|---:|
| `get_` | 60 | `mcp_` | 5 | `reload_` | 1 |
| `set_` | 33 | `btw_` | 4 | `plan_` | 1 |
| `terminal_` | 8 | `host_` | 4 | `guided_` | 1 |
| `pr_` | 7 | `queue_` | 4 | `tan` | 1 |
| `abort_` | 6 | `security_` | 4 | `omfg` | 1 |
| `live_` | 3 | `ssh_` | 2 | `manage_` | 1 |
| `collab_` | 3 | `switch_` | 2 | `synthesize_` | 1 |
| `cycle_` | 2 | `fork_` | 2 | `retry` | 1 |
| `list_` | 2 | `worktree_` | 2 | `clear_` | 1 |
| `browser_` | 2 | `compact` | 1 | `revive_` | 1 |
| `negotiate_` | 1 | `bash` | 1 | `write_` | 1 |
| `prompt` | 1 | `eval` | 1 | `import_` | 1 |
| `steer` | 1 | `dequeue` | 1 | `resume_` | 1 |
| `follow_` | 1 | `export_` | 1 | `marketplace_` | 1 |
| `new_` | 1 | `branch` | 1 | `delete_` | 1 |
| `drop_` | 1 | `redeem_` | 1 | `tui_` | 1 |
| `handoff` | 1 | `login` | 1 | `debug` | 1 |
| `logout` | 1 | `edit_` | 1 | `add_` | 1 |
| `update_` | 1 | `preview_` | 1 | `remove_` | 1 |
| `share_` | 1 | `fresh` | 1 | `move_` | 1 |
| `shake_` | 1 | | | | |

合计数校验：`60 + 33 + 8 + 7 + 6 + 5 + 4×4 + 3×2 + 2×7 + 1×42 = 197`；前缀数 `12 + 7 + 42 = 61`。

分布要点（复刻时的分模块依据）：

- `get_*` 60 条：只读查询，可全部走 60 s 超时的通用通路；其中 `get_available_models` / `get_providers` 慢（目录刷新）。
- `set_*` 33 条：写入会话或全局设置，多数立即回 `success`。
- `terminal_*` 8 条、`pr_*` 7 条、`mcp_*` 5 条、`collab_*`/`live_*` 各 3 条：fork 专属能力，stock sidecar 会 `Unknown command`。
- 单条命令中不成族的：`prompt`、`steer`、`bash`、`eval`、`compact`、`fresh`、`handoff`、`dequeue`、`branch`、`login`、`logout`、`debug`、`retry` 等（见 3.2）。

### 3.2 全量 197 条（按前缀分组，供对照实现）

```
negotiate: negotiate_protocol
prompt: prompt
steer: steer
follow: follow_up
abort: abort, abort_and_prompt, abort_retry, abort_bash, abort_eval, abort_subagent
new: new_session
drop: drop_session
get (60): get_state, get_available_commands, get_subagents, get_subagent_messages, get_available_models,
  get_queue, get_session_stats, get_branch_messages, get_config_warnings, get_advisor_status,
  get_last_assistant_text, get_messages, get_messages_page, get_login_providers, get_logout_accounts,
  get_usage, get_settings_schema, get_settings, get_providers, get_plan_mode,
  get_model_roles, get_model_role_metadata, get_skills, get_skill_detail, get_agent_definitions,
  get_hooks, get_mcp_servers, get_gui_themes, get_plugins, get_marketplaces,
  get_prompt_templates, get_memory_report, get_memory_overview, get_memory_activity, get_security_dashboard,
  get_security_scan, get_ssh_hosts, get_omp_update, get_context_report, get_active_tools,
  get_jobs, get_chat_config, get_force_tool, get_session_tree, get_themes,
  get_theme_colors, get_transcript, get_transcript_page, get_copy_targets, get_vibe_mode,
  get_goal, get_btw_history, get_loop_mode, get_command_arg_completions, get_plugin_detail,
  get_collab_state, get_directories, get_git_status, get_git_changes, get_git_diff
set (33): set_fast_mode, set_slow_mode, set_todos, set_host_tools, set_host_uri_schemes,
  set_subagent_subscription, set_event_filter, set_ask_dialog, set_model, set_thinking_level,
  set_steering_mode, set_follow_up_mode, set_interrupt_mode, set_auto_compaction, set_auto_retry,
  set_session_name, set_entry_label, set_setting, set_plan_mode, set_model_role,
  set_session_pinned, set_chat_config, set_prewalk, set_force_tool, set_vibe_mode,
  set_agents_paused, set_goal, set_loop_mode, set_skill_enabled, set_hook_enabled,
  set_plugin_enabled, set_plugin_features, set_plugin_setting
cycle: cycle_model, cycle_thinking_level
compact: compact
bash: bash
eval: eval
dequeue: dequeue
queue: queue_edit, queue_remove, queue_move, queue_clear
export: export_html
switch: switch_session, switch_leaf
branch: branch
fork: fork, fork_from
list: list_reset_credits, list_foreign_sessions
redeem: redeem_reset_credit
handoff: handoff
login: login
logout: logout
edit: edit_memory
update: update_memory
preview: preview_share_session
share: share_session
fresh: fresh
shake: shake_context
reload: reload_plugins
plan: plan_approval
guided: guided_goal
btw: btw, btw_branch, btw_cancel, btw_inject
tan: tan
omfg: omfg
manage: manage_skill
mcp: mcp_action, mcp_add, mcp_test, mcp_reauth, mcp_reauth_cancel
security: security_start, security_cancel, security_validate, security_set_disposition
ssh: ssh_manage, ssh_test
host: host_dictation_start, host_dictation_audio, host_dictation_stop, host_dictation_cancel
synthesize: synthesize_speech
retry: retry
clear: clear_context
revive: revive_subagent
write: write_local_paste
import: import_foreign_session
resume: resume_after_ask_reanswer
marketplace: marketplace_action
delete: delete_plugin_setting
live: live_start, live_stop, live_mute
terminal: terminal_open, terminal_run_cli, terminal_input, terminal_resize, terminal_close, terminal_list, terminal_snapshot, terminal_configure
tui: tui_open
browser: browser_host_start, browser_host_stop
debug: debug
collab: collab_start, collab_join, collab_leave
add: add_directory
remove: remove_directory
move: move_session
worktree: worktree_create, worktree_remove
pr: pr_repo, pr_list, pr_get, pr_diff, pr_draft, pr_create, pr_checkout
```

### 3.3 相关命令面（非 `RpcCommand`，但同一 wire 上有计数）

- `ExtensionUIRequest`：13 种 `method`（`select`/`confirm`/`askDialog`/`ask`/`input`/`editor`/`notify`/`setStatus`/`setWidget`/`setTitle`/`set_editor_text`/`open_url`/`cancel`）；`ExtensionUIResponse` 5 种。
- `AgentSessionEvent`：联合实际 **31** 个分支；源码注释写 `(24 types)` 是陈旧注释（`shared/rpc-types.ts:2505`），复刻时以联合为准。
- Stats HTTP：`VALID_PATHS` 17 条静态路径 + 动态 `/api/request/:id`；其中 `POST` 仅 `/api/sync`。

---

## 4. 复刻要点与陷阱（全部有上文证据）

1. **会话计数是"尾部近似"**：`messageCount` 只数 tail 32 KiB，UI 的"消息数"不是权威总数；需要精确数时只能全文件扫描或依赖 sidecar。
2. **标题槽定长 256 B**：GUI 只读文件前 256 B 并解析其第一行；title 行超过槽位会被截断，解析失败即 `null`。第二行才是 header，且 head 32 KiB 内读不到换行时需按 `MAX_HEADER_BYTES = 1 MiB` 重读。
3. **同一 id 多文件**：迁移期会出现两份同名会话，去重规则是 `modified` 倒序取首个；fork 复制 artifact 目录会让子代理 transcript 出现两份，需按"父文件"归属。
4. **`tabLayouts` 是数组**：每窗口一份，窗口顺序即数组顺序；兼容旧的单对象 `tabLayout`；`version` 必须严格 `=== 1`，否则整份丢弃。
5. **`ratio` 读取时 clamp**：`[0.2, 0.8]` 在 `sanitizePersistedTabLayout` 里做，不是只在写时。
6. **scheduler.db 的列很少**：筛选列只有 `state`/`review_state`/`created_at`/`task_id`，其余在 `data` JSON 里；`progress` 刻意不落库；`meta.notices` 是窗口通知列表（含已读状态）。
7. **models.yml 是文档级合并**：不能用 `JSON.parse` 后整体重写；必须 `parseDocument` + `setIn(["providers", id], entry)`，保留未渲染字段（`compat` 的其余键、`remoteCompaction`、`modelOverrides`、`effortMap`、`contextPromotionTarget`、`compactionModel`）。
8. **两个"主题"体系**：TUI 主题（`~/.omp/agent/themes`，`get_themes`/`get_theme_colors`）与 GUI 主题（内置 + `~/.omp/gui/themes`）互不相通；自定义选择值前缀是 `custom:`。
9. **quota 模块无实体**：`shared/quotas.ts` 只有排序/配色纯函数，`TrayQuota`/`TrayQuotaWindow` 定义在 `shared/ipc-types.ts`，不要为它单建表。
10. **`launch-profile` 的三张 flag 表必须成对搬运**：`DENYLISTED_FLAGS`（15）、`DENYLISTED_WITH_VALUE`（7）、`VALUED_FLAGS`（7）；只搬第一张会把 `--system-prompt "--session"` 这类合法值误删，或把 `--session <path>` 的值留成位置参数。
