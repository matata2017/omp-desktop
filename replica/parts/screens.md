# omp GUI 屏与对话框清单（recon slice）

源：`.research/omp-gui-git`（tag v0.9.35），只读。全部结论来自实际读过的文件，路径见文末 `### 证据`。

**计数：22 个 screen（S01–S22），54 个 dialog/overlay（D01–D54）。**

## 分类规则（先读这段，否则 S/D 边界会看起来矛盾）

1. **S** = 主窗口的常驻区域（shell、侧栏、标签栏、分屏窗格、输入区、dock、抽屉的 7 个视图）+ 由 Sidebar/TitleBar/命令面板打开、内容是"浏览或管理一类对象"、没有"确认/取消"语义的整窗目的地：Agent Hub、PR Center、Scheduled Tasks、Stats、Memory。
2. **D** = 一切以 `Modal`（`components/common/Modal.tsx`：portal + 焦点陷阱 + Esc 分层）或手写 portal/浮层呈现的会话式表面：设置及其家族窗口、选择器、提示、确认、审批、命令面板、toast。
3. 本仓库**没有第二个 OS 窗口的 renderer 入口**：所有 `*Window` 组件都在同一个 `App` 树里渲染（`App.tsx` 尾部）；真正的第二个 OS 窗口是 main 进程 `windowManager.createWindow` 再跑一份完整 App（`src/main/window.ts:97-120`）。"Window" 是命名习惯，不是 `BrowserWindow`。
4. 这是桌面应用：没有移动端布局，只有 `<1000px` 的紧凑断点（`App.tsx:370`、`PanelContainer.tsx` 的 `COMPACT_QUERY`）。因此"mobile"一列统一记 `n/a（仅 <1000px 紧凑态）`。

---

## 1. 屏 / 窗口（S01–S22）

| ID | screen | how you get there | purpose | key components | states seen |
|---|---|---|---|---|---|
| S01 | 主窗口外壳（App shell） | 应用启动（`main.tsx` → `<App/>`）；每个 OS 窗口一份实例 | 三栏骨架、全局快捷键分发、全部覆盖层宿主、IPC 事件接线 | `App.tsx` 的 surface：`Sidebar` + `TitleBar` + `TabBar` + `SidecarBanner` + `ConfigWarningsBanner` + `UpdateBanner` + `SplitWorkspace` + `PanelContainer`；另有 `ToastStack`、`AgentHoverCardLayer`、`LinkMenuHost`、`ExtensionDialog` | 侧栏开/关（⌘B）；inspector 默认关（⌘J 打开时落在 Overview）；sidecar `starting/ready/restarting/error/exited` → 横幅或静默；`configWarnings` 非空 → 可折叠黄条；更新 `available/downloading/ready/manual` → `UpdateBanner`；无活动 tab → `SplitWorkspace` 返回 `null`；<1000px 自动收起 inspector；有 `[role=dialog]` 或覆盖层时 Esc 不再中止回合 |
| S02 | 会话窗格 + 对话流（transcript） | 打开/新建 tab（⌘T；⇧⌘T = chat tab）；`SplitWorkspace` → `SessionPane` → `ChatStream` | 渲染消息、工具卡、思考块、流式输出、空态引导 | `ChatStream`、`MessageBubble`、`ProcessGroup`/`StreamingRows`、`ToolCard` + `components/tools/index.tsx` 的 `REGISTRY`（按工具名分发 40+ renderer，`GenericRenderer` 兜底）、`ThinkingBlock`、`TurnChangesRow`、`TodoSnapshotCard`、`TranscriptFindBar`、`TranscriptLoading` | `awaitingTranscript && rows=0` → loading 画面；空且非流式 → 品牌空态 + starter 卡（chat tab 文案不同）；流式 → 实时行 + "jump to latest"；单条消息 error 卡；compaction 折叠行 "show earlier"；切换会话 → 顶部不确定进度条；collab 只读 → 无编辑/分支按钮；⌘F 打开 find bar 并高亮匹配 |
| S03 | 分屏双窗格（split pair） | 把 tab 拖到会话区四边（`SplitDropOverlay`）或拖到已有 pair 的某一侧（`PaneDropOverlay`）；TabBar / Sidebar 的 split 操作 | 同一窗口并排/上下两个会话 | `SplitWorkspace`（`SessionPane`、`splitGrid`、divider）、`SessionRuntimeProvider` | 未分屏 / 分屏（每 pane 有 header：信号灯、标题、cwd、✕ 摘除）；非活动 pane 点击即激活并切换 tab；拖拽时四区或两区高亮；divider 可拖或用方向键（5px，步进 0.05）；已在屏上的 tab 不能当 drop 目标；chat tab 也能进分屏 |
| S04 | 输入区（composer） | 常驻会话下方（`SessionPaneBody`） | 输入/发送/流中 steer/排队/中止；附件、@file、/命令、听写、审批模式、思考级别、上下文策略 | `InputArea`、`use-composer-submit`、`use-completion-menu`、`use-dictation`、`ApprovalControl`、`ThinkingControl`、`ChatContextBar`/`ContextStrategyBar`、`DictationStrip`、`HistorySearchOverlay`、`ZoomableImage` | idle；发送中；流式中（Enter=steer，⌘↵=排队，按住修饰键显示预览）；提交不确定（`submissionUncertain`）；aborted；chat tab → 无审批/模式 chrome；大段粘贴 → 粘贴菜单；`@` / `/` 补全浮层；emoji 内联替换；听写中；queued 计数徽标；collab 只读禁用 |
| S05 | 执行状态 dock（Workspace dock） | 自动，位于 transcript 与 composer 之间 | 常驻展示 plan / todos / agents / queue / goal，替代原抽屉里的同名 tab | `WorkspaceDock`、`PlanDockCard`、`TodoDockCard`、`AgentsDockCard`、`QueueDockChip`、`GoalDockBar`、`WorkspaceDockFocus` | 无 plan/todo/agent/queue → 整块返回 `null`（高度 0）；summary 模式 `max-h min(40vh,320px)`；聚焦单卡后可用分隔条调高（持久化 `localStorage omp.dock.focusHeight`）；每卡独立 `PanelErrorBoundary`；chat tab 全部隐藏；流式期间 3s 轮询 `get_subagents` |
| S06 | 侧栏（Sidebar） | 默认可见；⌘B 或 TitleBar 左侧按钮切换 | 会话/项目导航 + 快捷入口 + 配额 | `Sidebar`、`SidebarQuotas`、`NotificationBell`、`LangSwitcher`、`WorkspaceDialog`、`ContextMenu`、`ConfirmDialog` | 首次加载骨架；`listError` → stale 提示 + retry；空态按 `code`/`work` 两种文案；分组（workspace 组 / `__chats__` / `__archived__`）；pin、就地 rename、删除确认、多选批量；流式/等待确认信号灯；quota 行；`mode=code|work`；导航项含 Command Center、Search、Memory（待复习徽标）、PR、Scheduled Tasks（未读点）、Usage、Stats、Hotkeys、Settings、主题 |
| S07 | 标签栏（TabBar） | `TitleBar` 下方常驻 | 多会话 tab、分屏 pair 合并 chip、拖拽排序、溢出可达 | `TabBar`、`TabSearchMenu`、`AgentPageChips` | 单 tab 隐藏关闭键；运行中关闭需 3s 内二次确认（`CONFIRM_CLOSE_MS=3000`）；pair 合并成一个 chip；chip 宽度 `compact/default/wide`；单行或最多三行；溢出后横向滚动 + "all tabs" 菜单；worktree tab 带分支图标，关闭前走 `WorktreeCloseDialog`；子代理页 chip 行 |
| S08 | Inspector 抽屉（PanelContainer） | ⌘J、TitleBar 右侧按钮、transcript 的 "Review"、打开文件、agent 需要时自动切 tab（`use-workspace-surfaces`） | 右侧 7 个视图：overview / agents / btw / diff / files / terminal / browser | `PanelContainer`、`PanelErrorBoundary`、`OverviewPanel`、`AgentRoster`、`BtwPanel`、`DiffPanel`、`FilesPanel`、`TerminalPanel`、`BrowserPanel` | 关闭（默认）；chat tab 仅 `overview/btw/files`；宽度 360–840（Browser ≤1600 且 ≤75% 窗口）；<1000px 或分屏时变右侧浮层；路由未 ready → `aria-busy` + `pointer-events-none`；单视图崩溃只 dead 该视图 |
| S09 | Overview 面板 | 抽屉 tab（`panel.tabs.overview`） | 会话现状一屏：上下文占用、工作目录与 git、会话改动、正在跑的东西、可用 MCP | `OverviewPanel`、`ContextBreakdown`、`useGitStatus`、`useWorkingTreeChanges`、`useContextReport` | 卡片级空态（无 git / 无变更 / 无运行 / 无 MCP）；chat tab 只显示 Context、Workspace、Running；改动 >6 条折成"看全部"；运行行 >3 条折成计数 |
| S10 | Changes / Diff 面板 | 抽屉 tab（`panel.tabs.diff`）；`ui.openChanges(scope)` 带 scope 打开 | 展示 edit/write/apply_patch 的 diff：仓库变更、本会话改动、时间线、虚拟产物 | `DiffPanel`、`RepositoryChanges`、`lib/diff` 的 `DiffView`/`parseDiff`、`workspace-ui` 的 `PanelEmpty`/`PanelToolbar` | `scope = repository|current|timeline|artifacts`；unified / split 切换；多文件时出现文件选择器；timeline 虚拟化长列表；无 diff → 空态；单文件 diff 失败 → 该行标 error 不拖垮整页 |
| S11 | Files 面板 | 抽屉 tab（`panel.tabs.files`）；`ui.openFilePreview(path,line)` 或文件链接 | 工作区文件树 + 抽屉内预览 + "@ 插入" | `FilesPanel`、`TreeView`、`MarkdownRenderer`、`PanelSearch`、`PathLink`、`fs:list` / `fs:read` IPC | 树上限 2000 文件、深度 8；预览 200 KB 截断并标注；loading / empty（无工作区）/ error；搜索本地过滤；`@` 按钮派发 `omp:insert-mention` |
| S12 | Terminal 面板 | 抽屉 tab（`panel.tabs.terminal`）；每个 tab 首次打开自动起 shell；agent `pty` 等待输入时自动前推 | xterm 终端（shell 与 agent 命令），多终端 chip | `TerminalPanel`、`@xterm/xterm` + `addon-fit`、`stores/terminals`、`terminal_*` RPC（`terminal_list/open/input/resize/close/snapshot`） | 空（无终端 → `PanelEmpty` + 新建）；运行中；`busy` / `input` 状态点；已退出（标题删除线）；`root` 标记；关闭全部后同 tab 不再自动新建。注：`terminal_*` 只在 fork 版 sidecar 存在（`evidence: 推断`，来自计划的 fork 命令面） |
| S13 | Browser 面板 | 抽屉 tab（`panel.tabs.browser`）；agent 打开页面时自动前推 | 内嵌网页：多页 chip、前进/后退/刷新/外部打开、页内查找 | `BrowserPanel`、`BrowserFindBar`、`stores/browser`、`browser:*` IPC | 空（无页面）；页面加载中 / title+URL；页面是原生 view 覆盖在 renderer 之上，任何 dialog/menu/toast 覆盖其矩形时切换为静态截图（`OVERLAY_SELECTOR` + `MutationObserver`）；find bar 打开；agent 触发的页面自动 reveal。fork 专属（`evidence: 推断`，同上） |
| S14 | Agents 面板（roster） | 抽屉 tab（`panel.tabs.agents`）；tab 上有未读失败点 | 监控子代理：本轮 / 全部，列表 / DAG 图，暂停/停止/展开 | `AgentRoster`、`SubagentDag`、`useAgentProjection`、`AgentHoverCard` | 无子代理 → 空态；`turn`/`all` 范围；`list`/`graph` 视图（按 tab 记忆）；失败未读点（error 优先）；行点击打开 `AgentPage`；打开即把失败标记已读；流式期间 3s 轮询 |
| S15 | BTW 面板（side questions） | 抽屉 tab（`panel.tabs.btw`）；`/btw` 另开 modal 版 | 本会话的旁问主题列表 + 选中线程 + 追问 composer | `BtwPanel`、`BtwAnswer`、`stores/btw` | 空（无主题）；loading；线程 `running/complete/cancelled/error/interrupted` 徽标；流式跟随直到用户滚动；提问失败 inline 错误；分支（branch）按钮 |
| S16 | 子代理页（顶替对话） | 点击 dock / 侧栏 / roster 里的子代理行（`agent-pages.show`） | 用会话同款列与组件渲染该子代理 transcript；composer 位置换成只读 agent bar | `AgentPage`、`ChatStream`（`ReadOnlyTranscriptContext`）、`agent-page-runtime`、`AgentTitle`/`AgentCrumbs` | 运行中/等待/失败/完成/停止信号；只读 transcript；agent bar 的 stop/revive；页面属某个 session，切 session 自动丢弃；顶部面包屑可回到会话 |
| S17 | 子代理页（并排） | `agent-pages.beside`（`AgentBeside`） | 同一 tab 内左对话右子代理页 | `AgentBeside`、`AgentPage(beside)` | 默认 50/50，divider 可拖或 ←/→（夹在 0.2–0.8）；无页面时回到单栏；两栏各自独立滚动 |
| S18 | Agent Hub 窗口 | ⌥A；侧栏导航；命令面板 | 子代理控制中心：定义（模型/思考级别/prewalk 覆盖）与运行中实例 | `AgentHubWindow`（`Modal size="lg"`，`h-[70vh]`）、`agent-hub-settings`、`SubagentTranscript`、`AgentTranscriptDrawer` | tab `definitions` / `hub`；定义加载中/错误/空；运行中计数徽标；抽屉打开子代理 transcript 时 Esc 只关抽屉 |
| S19 | PR Center 窗口 | ⌥P；侧栏导航；命令面板 | GitHub PR：列表（CI 徽标、diff 统计）、详情 markdown、按文件懒加载 diff、AI 起草创建、一键 checkout 到 worktree tab | `PrCenterWindow`（`Modal size="full"`）、`PrDetailPane`、`PrCreateDialog`、`stores/pr-center` | 探测中（spinner）；不可用（`gh_missing` 等，给安装提示）；列表 loading/empty/error+retry；选中项详情；创建按钮在不可用时 disabled 并带原因 |
| S20 | Scheduled Tasks 窗口 | 侧栏 "Scheduled Tasks"（有待审时直达 review）；菜单；命令面板；通知 | 定时任务：任务、运行、审阅、洞察 + 健康状态 + 全局暂停 | `TasksWindow`（`Modal size="full"`）、`TasksView`、`RunsView`/`ReviewView`、`InsightsView`、`TaskEditorDialog`、`stores/scheduler` | 4 个视图 tab（runs/review 带计数徽标）；健康 pill：`healthy`（下次触发倒计时）/`paused`/其它；无任务空态；运行中/排队；洞察按 7/30/90 天，含成功率与成本 |
| S21 | Stats dashboard | 侧栏 "Stats"；TitleBar 动作菜单；命令面板 | 本地统计：10 个视图（overview/models/providers/tools/costs/errors/frustration/gain/projects/requests）+ 时间范围 + 同步 | `StatsDashboard`（`Modal size="full"`）、`stats/*Route.tsx`、`stats/shared.tsx` 的 `RouteFrame`/`MetricCard`/`StatTable` | 打开即静默 sync；手动 sync 成功/后台/不可用/错误 toast；范围 `1h/24h/7d/30d/90d/all`；每路由 loading/error（带 retry）/empty（该范围无数据）/stale |
| S22 | Memory 窗口 | ⌥⇧M；侧栏 "Memory"（有待复习直达 skills）；命令面板 | 项目记忆：知识、来源、自学技能；可纠正/遗忘 | `MemoryWindow`（`Modal size="lg"`）、`KnowledgeView`、`SourcesView`、`SkillsView`、`MemoryIntro`/`OtherBackend`、`useMemoryProject` | tab `knowledge/skills/sources`（默认 knowledge）；状态行 `off/reading/consolidating/failed/empty/current`；项目选择器（含 work 工作区）；非内置后端 → `OtherBackend` 提示；技能待复习徽标 |

---

## 2. 对话框 / 模态 / 浮层（D01–D54）

| ID | screen | how you get there | purpose | key components | states seen |
|---|---|---|---|---|---|
| D01 | 设置（Settings，**全屏 portal**） | ⌘,（⌃,）；侧栏 Settings；命令面板；菜单 `open-settings` | schema 驱动的 agent 设置 + 产品级页（Interface、Capabilities、Skills、MCP、Plugins & resources、Hooks、Commands、Security Center、SSH Hosts、Updates、Agent advanced，以及 schema 生成的模型/任务/工具/记忆页） | `SettingsWindow`（`createPortal`，`role="dialog"`，自管 Esc，非 `Modal`）、`settings-window-model.ts`、`SchemaTabContent`、`CapabilitiesHome`、`AdvancedTab`、`SkillsSettingsPage`、`SecuritySettingsPage`、`SshSettingsPage`、`UpdatesSettingsPage`、`ExtensionSettingsPage`、`InventorySettingsPage`、`ProviderConfigDialog` | 左侧分组导航 + 全局搜索；`LoadState = loading|error|ready`；逐项即时写 `set_setting`；模型/Provider 类字段渲染为可搜索下拉；launch profile 文本字段与校验；窄屏单列（`omp-column-settings`） |
| D02 | Providers 窗口 | 设置 → Providers；命令面板；TitleBar 动作 | Provider 与登录：列表、登录/登出、自定义 provider 增改、搜索（>8 行出现过滤框） | `ProvidersWindow`（`Modal size="lg"`，`max-h-70vh`）、`ProviderConfigDialog`、`ConfirmDialog` | loading/error；未认证 / 已认证 / disabled；登出确认；登录进行中（`LongTaskStatus`）；空列表 |
| D03 | Usage & quotas 窗口 | 侧栏 "Usage"；TitleBar 动作；命令面板 | Provider 配额条 + 重置倒计时 + 本会话 token/成本 + reset credits 兑换 | `UsageWindow`（`Modal size="lg"`）、`ProgressBar`、`quota-rows`、`ConfirmDialog` | 配额加载/错误/空；`exhausted`/`warning` 徽标；倒计时；本会话统计随回合更新；兑换需确认 |
| D04 | Model roles 窗口 | 命令面板；设置模型页 | 给每个 model role 指定模型与思考级别，候选由后端提供 | `ModelRolesWindow`（`Modal size="lg"`）、`AsyncSection`、`ModelValueSelect` | 加载/错误（可 retry）；已选 / 继承（`inherit`）；新模型不支持原思考级别 → 提示"级别已丢弃"；无候选空态 |
| D05 | Model compare 窗口 | 命令面板；设置 | 全模型矩阵：auth、上下文窗口、$/1M、provider 配额、角色分配、当前模型 | `ModelCompare`（`Modal size="full"`）、`buildModelRows` | 可排序/过滤；行点击 `set_model`；行内 role picker；数据缺失列显示 "—"；roles/usage 拉取失败时降级不丢行 |
| D06 | Extensions 窗口 | 命令面板 `openExtensions(tab)`；设置同名页 | hooks / MCP servers / custom slash commands 的紧凑管理窗（兼容壳） | `ExtensionsPanel`（`Modal size="lg"`，`h-[70vh]`）、`McpServerCard`、`McpServerWizard`、`McpFeedback` | 每 tab 首次懒加载 + 缓存 + 手动刷新；搜索过滤；行内开关乐观更新失败回滚 + toast；MCP 动作菜单（enable/disable/reconnect/remove/test/reauth）；hook 改动"下次启动生效"提示；commands tab 只读 |
| D07 | Inventory 窗口 | 命令面板 `openInventory(tab)`；设置 resources 页 | plugins / marketplaces / prompt templates / memory 报告 | `InventoryPanel`（`Modal size="lg"`）、`MarketplacesSection`、`PluginDetailDrawer` | 每 tab 懒加载 + 缓存；plugins 行开关乐观更新；行点击开详情抽屉（Esc 只关抽屉）；marketplace 添加/安装；templates、memory 只读；加载/错误/空 |
| D08 | Modes 窗口 | 命令面板 `openModes(tab)` | Vibe / Goal / Loop 三种会话模式 | `ModesPanel`（`Modal size="lg"`，`h-[70vh]`）、`useModeRpc` | 每 tab 首次加载、再激活时静默重取；提交中保留旧值；失败 inline + Retry；`goal_updated` / `loop_mode_update` 事件就地更新 |
| D09 | 命令面板 | ⌘K / ⌃K | 模糊搜索命令注册表并执行（动作、切换、picker、窗口、子菜单、prompt） | `CommandPalette`（手写 overlay，`role="dialog"`，顶部 `pt-[12dvh]` 锚定）、`lib/command-registry` | 空查询（分组列表）；有查询（模糊评分）；当前 tab 不可用的命令 disabled 且不可被选中；需要参数的命令把 `/xxx` 交回 composer；RPC 拉取失败 → 行内错误 + retry；Esc 在任意焦点位置都关 |
| D10 | 模型选择器 | ⌥M；composer 的模型按钮；命令面板 | 按 provider 分组、可折叠、可搜索地选模型，显示认证状态 | `ModelPicker`（`Modal size="picker"`，chromeless，top 锚定） | 空搜索；搜索无结果；provider 折叠/展开；未认证 provider 提示；当前模型高亮；选择即 `set_model` |
| D11 | 会话选择器（resume） | 侧栏搜索按钮；命令面板；菜单 | 恢复磁盘上的历史会话：字面匹配优先，再模糊，再全文检索 | `SessionPickerDialog`（`size="picker"`，chromeless，top）、`use-session-list` | 空列表；搜索中；无匹配；scope `本目录 / 所有项目`（Tab 切换）；排序与路径显示选项；错误 |
| D12 | 会话切换（忙时） | 会话正在流式/压缩时打开另一个会话（自动） | 避免原地切换中止运行：推荐"新 tab 打开"，另给新窗口与中止切换 | `SessionSwitchDialog`、`use-session-switch` | 流式中/压缩中触发；三选一；取消保持原状；确认后按选择建 tab / window 或 abort |
| D13 | 模型恢复 | 打开某会话时其保存的模型已不可用（自动） | 让用户决定：用当前模型打开，还是留在原地 | `ModelRestoreDialog`、`stores/session` | 模型缺失（provider 被移除/登出/下线）；两按钮；不发隐式切换 |
| D14 | 分支选择器 | 命令面板；菜单 | 列出当前会话的用户消息，从选中项开分支 | `BranchPickerDialog`（`size="picker"`，chromeless，top）、`getBranchMessages` / `branch` | loading；空（无可分支消息）；搜索；选中即 `branch(entryId)` 并重建会话 |
| D15 | 会话树（分支图） | 命令面板 `openSessionTree`；菜单 | 可视化分支树：节点、fork 边、HEAD、活动分支；平移/缩放/拖拽节点 | `SessionTreeDialog`（`Modal size="full"`）、`SessionTreeNodeCard`、`session-tree-layout.ts` | 有 `get_session_tree` → 富树；否则退化为 `get_branch_messages` 单链；空态；过滤（all/当前分支/仅用户/仅标签）；标签编辑；↑↓/Enter/L 键盘操作 |
| D16 | 会话信息 | TitleBar 动作；命令面板 | 原生渲染 `getSessionStats`：身份、消息/工具计数、token 拆分、premium 请求、成本、上下文占用 | `SessionInfoDialog` | loading；error；正常数据；无统计时空值 |
| D17 | 重命名会话 | TitleBar 标题；侧栏行菜单；命令面板 | 单行输入改名，写回并同步 store | `RenameSessionDialog` | 预填当前名；空值/失败 toast；流式中受限（由调用方禁用） |
| D18 | Handoff | 命令面板；菜单 | 让 agent 把当前会话总结成 handoff 文档并在新会话继续（保留原会话） | `HandoffDialog`、`rpc.handoff(instructions?)` | 可填自定义指令；流式中不可用并 inline 说明原因；失败 toast |
| D19 | 新建 worktree tab | ⌥T；TabBar / Sidebar 菜单 | 建 `omp/gui/<slug>` 分支并检出到 `~/.omp/wt/...`，在该目录开 tab | `WorktreeDialog` | 名称校验/冲突错误；预览只显示分支名；创建中；成功即开新 tab |
| D20 | 关闭 worktree tab | 关闭 worktree 绑定的 tab（自动，先于 `closeTab`） | 检查该 tab 自己的 sidecar 是否干净，决定删除/保留 | `WorktreeCloseDialog` | clean → 删除并关闭 / 保留并关闭 / 取消；dirty → 计数 + 强制删除（丢弃改动）/ 保留 / 取消 |
| D21 | 工作区切换（Workspace manager） | TitleBar 项目名；侧栏菜单 | 列出所有已知工作区（会话 cwd + 当前项目），跳转或新增目录 | `WorkspaceDialog`、`window.omp.sidecar.selectProject/setProject` | 当前项高亮；空（无历史）；新增走原生目录选择；切换失败 toast |
| D22 | 工作区目录（/dirs） | 命令面板；菜单 | 列会话根目录、加/删根、把会话 cwd 迁到别的目录 | `WorkspaceDirsDialog` | 主根徽标；loading/error；删除需 inline 确认；移动走目录选择 + `move_session` + 重载；流式中禁止变更 |
| D23 | 分享会话 | TitleBar 动作；命令面板 | 预览上传内容再生成分享链接 | `ShareSessionDialog`、`preview_share_session` / `share_session` | sidecar 未就绪 → 错误；预览 loading/error；上传中；成功后显示链接 + 复制/打开 |
| D24 | 导入外部会话 | 命令面板；菜单 | 从 Claude/Codex 复制会话为新的 OMP 会话（不改源） | `ImportForeignDialog` | 源列表加载；多选；单条导入 → 直接切换；多条 → 侧栏新增 + 计数 toast；错误 |
| D25 | 全屏 composer 编辑器 | ⌃G | 用 CodeMirror 编辑草稿（markdown、历史、软换行），⌘↵ 写回 | `ComposerEditorDialog`（`Modal size="full"`）、CodeMirror | 有内容/空；未保存关闭 → 二次 Esc 丢弃保护；"外部编辑器打开"往返；写回失败提示 |
| D26 | BTW（旁问）对话框 | 输入 `/btw ...` 或命令触发（`ui.openBtw`） | 单个旁问的问答表面，可分支进主会话 | `BtwDialog`、`BtwAnswer`、`stores/btw` | 提问中；流式答案；已完成；取消/错误/中断徽标；同一时刻只允许一个 running；分支按钮 |
| D27 | Collab（协作） | 命令面板；deep link 加入链接 | 起/加入协作 relay，显示参与者与只读状态 | `CollabDialog`、`collab_*` RPC | 空闲；busy（join/leave）；relay 地址与 join link 复制；错误 inline；加入后为只读则主界面禁输入 |
| D28 | Live 语音 | 命令面板；菜单 | 实时语音会话：麦克风开关、转写滚动、挂断 | `LiveVoiceDialog`、`lib/live-state`、`onLiveFrame` 事件 | 连接中；通话中（静音/取消静音）；转写流；错误；挂断后重置 |
| D29 | Debug 控制台 | 命令面板 `openDebug`；菜单 | `debug` RPC 的结构化前端：会话条、动作栏、结果区、原始 JSON 编辑器 | `DebugConsoleDialog`（`Modal size="lg"`）、`debug` RPC | 无调试会话；会话列表自动刷新；动作执行中；结果/错误；adapter 缺失提示；原始请求 JSON 折叠 |
| D30 | 应用日志 | TitleBar 动作；命令面板；`openLogs` | 应用日志查看（诊断，不进抽屉） | `LogsDialog`（`Modal size="lg"`，body `p-0`）+ `LogPanel` | 空；追尾中；错误；关闭即卸载 |
| D31 | 复制目标选择器 | 命令面板；快捷键 | 列出服务端提供的 copy targets（会话/片段），选一个复制到剪贴板 | `CopySelectorDialog`、`get_copy_targets`、`flattenCopyTargets` | sidecar 未就绪 → `common.notConnected`；loading；空列表；复制成功态；错误 |
| D32 | 上下文报告（/context） | TitleBar 动作；命令面板；`/context` | 单条堆叠条 + 图例：各类别 token 与占比；有窗口时含 auto-compact 缓冲 | `ContextReportDialog`、`get_context_report` | 未知 context window → 去掉所有百分比只显示构成；loading；error；空 |
| D33 | 活动工具（/tools） | TitleBar 动作；命令面板 | 当前 agent 可见的全部工具，按来源分组（内置 / `mcp__` / 扩展 / 插件）+ 搜索 | `ActiveToolsDialog`（`Modal size="lg"`）、`get_active_tools` | loading；error；空；搜索过滤；分组折叠 |
| D34 | 异步任务（/jobs） | 命令面板；菜单 | 会话的后台 job：running 优先、再 recent；有 job 树时按子代理嵌套 | `JobsDialog`（`Modal size="lg"`）、`JobWidgetCard` | 无 job 空态；running/parked/done/failed；widget 视图（spark/meter/bars/stat/grid/text）；updatedAt 时间；错误 |
| D35 | 变更日志 | 命令面板 `/changelog` | 渲染打包进 bundle 的 `CHANGELOG.md` | `ChangelogDialog`（`Modal size="lg"`）、`MarkdownRenderer` | 纯静态；markdown 由 sanitizer 兜底 |
| D36 | 强制工具（/force） | 命令面板；菜单 | 让下一回合强制走某个工具，可带 prompt；可清除当前强制 | `ForceToolDialog`（`Modal size="picker"`）、`get_force_tool` / `get_active_tools` / `set_force_tool` | 当前已强制 → 显示 + 清除；工具列表 loading/error/空；应用后立即发送可选 prompt |
| D37 | 快捷键参考 / 改键 | ⌘/（⌃/）；命令面板 | 可搜索的快捷键表，支持逐行重绑与冲突检测 | `HotkeysDialog`、`lib/keymap`（`KEYMAP_ACTIONS`、冲突检测） | 默认/已覆盖；录制新 chord；冲突 error（阻断保存）/ warning（覆盖生效）；保留 chord（输入框/原生）标注 |
| D38 | 首次运行引导 | 首次启动且无可用模型/provider（自动） | 引导配置 provider/登录，或写自定义 provider 配置样例 | `FirstRunOnboardingDialog`（`Modal size="lg"`，`max-h-[86vh]`）、`hasUsableModelProvider` | 可用 provider 已存在 → 不显示；登录中；自定义 YAML 示例；sidecar 未就绪时按钮态 |
| D39 | 主题选择器 | 侧栏底部按钮；命令面板 | 搜索式主题列表：内置主题、"system"、`~/.omp/gui/themes` 自定义；选中即应用并持久化 | `ThemePickerDialog`（`size="picker"`，chromeless，top）、`lib/themes` | 列表/搜索无结果；自定义主题损坏 → 显示原因且不可选；当前项高亮；即时换肤 |
| D40 | 计划审批 | agent 写 `xd://propose` 且 plan mode 开启（自动，`plan_proposal` 事件） | 展示计划 markdown，按后端给出的选项审批 | `PlanApprovalDialog`（`Modal size="lg"`，零 props 自订阅）、`stores/plan-approval` | 无提案 → 不渲染；approve execute / approve compact / keep context / refine（需反馈文本）/ dismiss；Esc 关闭；新提案覆盖旧的；保存计划走原生保存对话框 |
| D41 | 扩展 UI 分发（含工具审批） | sidecar 发 `extension_ui_request`（自动；`ExtensionDialog` 常驻 App） | 把最老的待处理请求渲染成 select / confirm / input / editor / open_url；工具审批走专用 `ApprovalDialog`；`notify` 变 toast | `ExtensionDialog`、`ApprovalDialog`、`AskDialog`、sign-in 浏览器面板、`extension-ui` store | 队列空 → 不渲染；多请求按序；倒计时超时自动取消；工具名 + 能力徽标 + 等宽参数预览；Approve / 本次会话 / 记住文件/目录 / Deny（关闭=Deny）；多问题 ask（多选/单选 + 自定义答案）；登录浏览器步骤；widget 浮层 |
| D42 | 会话内容搜索 | ⇧⌘F | 在任意会话的对话文本里搜（不含工具输出），按会话分组，选中跳到该消息 | `ConversationSearchDialog`、`sessions.searchContent` | 空查询；无结果；索引未就绪 → 自动重问；结果分组 + 高亮片段；键盘上下选择 |
| D43 | 反馈 | 命令面板；错误面 `openFeedback(prefill)` | 组装 GitHub issue（标题/正文/labels），让用户到网页提交；诊断信息可选并预览 | `FeedbackDialog`（`Modal size="lg"`） | 预填错误文本；勾选环境/最近错误 → 正文预览；打开浏览器后不自动提交 |
| D44 | 模型 benchmark | 命令面板；设置 | 对一组模型跑 benchmark：profile、runs、并行度，结果矩阵 | `BenchmarkDialog`（`Modal size="lg"`）、`bench:*` IPC | 未运行；运行中（可取消）；成功表格；失败显示 stderr；模型选择来自 `ModelValueSelect` |
| D45 | 定时任务编辑器 | Tasks 窗口 "New"/编辑（scheduler store） | 创建/编辑任务：cron/间隔、prompt、工具白名单、通知、审阅 | `TaskEditorDialog`（`Modal size="lg"`）、`TaskForm`、`useDraftProblems` | 新建/编辑；草稿校验错误；保存中；关闭丢弃确认 |
| D46 | Provider 配置（自定义） | Providers 窗口 "Add" / 行编辑 | 自定义 provider 表单 + 模型表编辑；删除确认 | `ProviderConfigDialog`（`Modal size="lg"`；内嵌删除 `Modal size="sm"`）、`ModelEditor`、`ProviderConfigRow`、`FormFields` | `view.kind = list/form`；字段错误；保存中；删除二次确认；配置解析错误 |
| D47 | 创建 PR | PR Center 的 "+" | 以 AI 起草的标题/正文创建 PR | `PrCreateDialog`（`Modal size="lg"`） | 草稿生成中；生成失败；提交中；base/head 选择；成功关闭并刷新列表 |
| D48 | MCP server 向导 | Extensions → MCP → "+" | 添加 MCP server（stdio / http），测试连接 | `McpServerWizard`（`Modal size="md"`）、`McpServerWizardForm` | 表单校验；测试中；测试结果（前 10 个工具 + "+N more"）；添加失败 |
| D49 | 通用确认框 | 各处破坏性操作（删除会话/组、登出、兑换、卸载等） | "真的要这样吗"：安全按钮自动聚焦，Enter 不会误确认 | `ConfirmDialog`（`Modal size="sm"`）、`Button variant="danger"` | 正文 + 可选警告行；busy（操作中禁用）；取消/确认 |
| D50 | Toast 通知栈 | 任意操作结果 | 右上角非阻塞通知；浏览器页面打开时向左让位或被视为遮挡 | `ToastStack`（portal）、`stores/toast` | `info/success/warning/error`；自动过期（500ms 剪枝）；手动关闭；多条堆叠；浏览器面板占位时改道 |
| D51 | 输入历史搜索 | ⌃R（composer 内） | 对已发送 prompt 做 token-AND 模糊过滤，↑↓ 选择，Enter 填回 | `HistorySearchOverlay`、`stores/input-history` | 空历史；无匹配；当前工作区 / 所有工作区切换；最多 50 条；选中即插入 |
| D52 | 对话内查找条 | ⌘F（transcript 聚焦）/ 菜单 Edit→Find | 在已渲染的 transcript 行里查找并高亮，↑/↓ 步进 | `TranscriptFindBar`、`findTranscriptMatches`、`paintFindHighlights` | 无匹配（0/N）；有匹配 + 当前项；关闭还原高亮；跨虚拟化行滚动定位 |
| D53 | 全部标签菜单 | TabBar 的 "all tabs" 按钮；chip 的 "Split with…" | 搜索式到达被滚出视口的 tab，或选分屏伙伴；pair 占一行 | `TabSearchMenu`（portal，坐标锚定） | 搜索过滤；当前 tab 打勾；pair 行显示轴（columns/rows）；Esc / 点外关闭 |
| D54 | 子代理悬浮卡 | 悬停或键盘聚焦子代理行 400ms | 不打开页面就看一眼：状态、now-line、最近 3 步 | `AgentHoverCard` / `AgentHoverCardLayer`（全局单层 portal）、`useTabSubagent` | 400ms 延迟出现、150ms 延迟消失；换行立即切换；Esc 隐藏；键盘聚焦同效；行滚出/卸载即消失 |

---

## 不明确

1. **"Window" 命名 vs 真实窗口**：`SettingsWindow`、`UsageWindow`、`ProvidersWindow`、`ModelRolesWindow`、`AgentHubWindow`、`PrCenterWindow`、`TasksWindow`、`MemoryWindow`、`InventoryPanel`、`ExtensionsPanel`、`ModesPanel` 全部渲染在同一 renderer 树里；只有 main 的 `windowManager.createWindow`（`src/main/window.ts:97-120`）会开真正的第二个 OS 窗口，且它加载的是同一份 App。因此"独立窗口"在本清单里按 UI 表面计，不按进程/窗口计。
2. **Settings 的归类**：它是全屏 `createPortal`（不是 `Modal`，自管 Esc），语义上更像"屏"；本表按任务要求把它放在 D01，与其它配置窗口同族。若按"整屏目的地"重排，它应升为 S。
3. **Benchmark / ModelCompare**：从设置与命令面板都能到，但都作为 App 顶层 overlay 挂载；算"设置家族"还是"独立工具"没有代码层面的答案，我按"设置家族"放进 D。
4. **`TaskEditorDialog`**：由 `useSchedulerStore` 而非 `ui` store 驱动，不属于 Tasks 窗口的 tab；按 dialog 计。
5. **`AgentPageChips`**：渲染在 TabBar 内，是 tab 条的一部分还是子代理页的独立入口，边界模糊；归入 S07 描述，不单列 ID。
6. **`SubagentTranscript` / `SubagentDag` / `JobWidgetCard` / `SessionTreeNodeCard`**：都是被父表面复用的子组件，不单列 ID。
7. **通用原语**：`Modal`、`ContextMenu`、`LinkMenuHost`、`Tabs`、`AsyncSection` 是构件而非表面；只有 `ConfirmDialog` 与 `ToastStack` 因"每个功能都要复刻其状态机"而单列。
8. **移动端**：无移动布局，只有 `<1000px` 紧凑断点（抽屉转浮层、inspector 自动收起）。所有"mobile"因此记 `n/a`，不是遗漏。
9. **fork 专属面板**（Terminal / Browser / Jobs / 设置 schema）依赖 sidecar 能力探测；本表只描述 renderer 侧表面，运行期是否可见由 `AppState.caps` 决定（`evidence: 推断`，来自计划的能力探测表，非本仓库代码）。

---

### 证据

> 一行一个 ID；`path` 为该表面自身的实现（必要时附挂载点）。

S01 — `src/renderer/App.tsx`（surface 组装；`{sidebarVisible && <Sidebar/>}` 在 845 行，`<main>` 848 行起）；`src/renderer/main.tsx`；`src/renderer/index.html`；`src/main/window.ts:97-120`
S02 — `src/renderer/components/chat/ChatStream.tsx`（空态 798-830、行渲染 862-900）；`src/renderer/components/tools/index.tsx:113-165`；`src/renderer/components/layout/SplitWorkspace.tsx:150-200`
S03 — `src/renderer/components/layout/SplitWorkspace.tsx:1-175`（`SplitDropOverlay`、`PaneDropOverlay`、`SessionPane`）
S04 — `src/renderer/components/layout/InputArea.tsx:1-1240`；`src/renderer/components/layout/ApprovalControl.tsx`；`src/renderer/components/layout/ChatContextBar.tsx`
S05 — `src/renderer/components/chat/dock/WorkspaceDock.tsx:1-163`；`src/renderer/components/chat/dock/{PlanDockCard,TodoDockCard,AgentsDockCard,QueueDockChip,GoalDockBar}.tsx`
S06 — `src/renderer/components/layout/Sidebar.tsx:240-400,1190-1300,1350-1480`；`src/renderer/components/layout/SidebarQuotas.tsx`；`src/renderer/components/layout/NotificationBell.tsx`
S07 — `src/renderer/components/layout/TabBar.tsx:1-83`；`src/renderer/components/layout/TabSearchMenu.tsx`
S08 — `src/renderer/components/layout/PanelContainer.tsx:34-60,120-215`
S09 — `src/renderer/components/panels/OverviewPanel.tsx:1-83`
S10 — `src/renderer/components/panels/DiffPanel.tsx:1-73`；`src/renderer/components/panels/RepositoryChanges.tsx`
S11 — `src/renderer/components/panels/FilesPanel.tsx:1-73`
S12 — `src/renderer/components/panels/TerminalPanel.tsx:1-73`；`src/renderer/stores/terminals.ts:94-296`
S13 — `src/renderer/components/panels/BrowserPanel.tsx:1-83`；`src/renderer/components/panels/BrowserFindBar.tsx`
S14 — `src/renderer/components/agents/AgentRoster.tsx:1-93`；`src/renderer/components/panels/SubagentDag.tsx`
S15 — `src/renderer/components/panels/BtwPanel.tsx:1-63`；`src/renderer/components/panels/BtwAnswer.tsx`
S16 — `src/renderer/components/agents/AgentPage.tsx:1-63`；`src/renderer/components/layout/SplitWorkspace.tsx:150-175`
S17 — `src/renderer/components/agents/AgentBeside.tsx:1-73`
S18 — `src/renderer/components/panels/AgentHubWindow.tsx:56-60,678-700,984-1025`
S19 — `src/renderer/components/panels/PrCenterWindow.tsx:1-215`；`src/renderer/components/panels/pr/{PrCreateDialog,PrDetailPane}.tsx`
S20 — `src/renderer/components/tasks/TasksWindow.tsx:1-123`；`src/renderer/components/tasks/{TasksView,RunsView,InsightsView,TaskEditorDialog}.tsx`
S21 — `src/renderer/components/stats/StatsDashboard.tsx:1-143`；`src/renderer/components/stats/shared.tsx:1-83`；`src/renderer/components/stats/*Route.tsx`
S22 — `src/renderer/components/memory/MemoryWindow.tsx:1-123,336-395`；`src/renderer/components/memory/{KnowledgeView,SourcesView,SkillsView,MemoryIntro}.tsx`

D01 — `src/renderer/components/settings/SettingsWindow.tsx:926-1043`；`src/renderer/components/settings/settings-window-model.ts:33-140`
D02 — `src/renderer/components/settings/ProvidersWindow.tsx:1-60,401-600`
D03 — `src/renderer/components/settings/UsageWindow.tsx:1-63,444-575`
D04 — `src/renderer/components/settings/ModelRolesWindow.tsx:1-63,431-480`
D05 — `src/renderer/components/settings/ModelCompare.tsx:1-21,655-723`
D06 — `src/renderer/components/panels/ExtensionsPanel.tsx:1-63,982-1000`
D07 — `src/renderer/components/panels/InventoryPanel.tsx:1-50,966-972`；`src/renderer/components/panels/inventory/*`
D08 — `src/renderer/components/panels/ModesPanel.tsx:1-60,759-777`
D09 — `src/renderer/components/dialogs/CommandPalette.tsx:100-200,684-700`
D10 — `src/renderer/components/dialogs/ModelPicker.tsx:1-30,265-272`
D11 — `src/renderer/components/dialogs/SessionPickerDialog.tsx:1-47,224-231`
D12 — `src/renderer/components/dialogs/SessionSwitchDialog.tsx:1-40`
D13 — `src/renderer/components/dialogs/ModelRestoreDialog.tsx:1-25`
D14 — `src/renderer/components/dialogs/BranchPickerDialog.tsx:1-30,145-160`
D15 — `src/renderer/components/dialogs/SessionTreeDialog.tsx:1-45,718-730`；`src/renderer/components/dialogs/session-tree-layout.ts`
D16 — `src/renderer/components/dialogs/SessionInfoDialog.tsx:1-20`
D17 — `src/renderer/components/dialogs/RenameSessionDialog.tsx:1-15`
D18 — `src/renderer/components/dialogs/HandoffDialog.tsx:1-30`
D19 — `src/renderer/components/dialogs/WorktreeDialog.tsx:1-30`
D20 — `src/renderer/components/dialogs/WorktreeCloseDialog.tsx:1-28`
D21 — `src/renderer/components/dialogs/WorkspaceDialog.tsx:1-20`
D22 — `src/renderer/components/dialogs/WorkspaceDirsDialog.tsx:1-20`
D23 — `src/renderer/components/dialogs/ShareSessionDialog.tsx:1-60`
D24 — `src/renderer/components/dialogs/ImportForeignDialog.tsx:1-30,180-186`
D25 — `src/renderer/components/dialogs/ComposerEditorDialog.tsx:1-20,133-140`
D26 — `src/renderer/components/dialogs/BtwDialog.tsx:1-43,121-130`
D27 — `src/renderer/components/dialogs/CollabDialog.tsx:1-63,155-165`
D28 — `src/renderer/components/dialogs/LiveVoiceDialog.tsx:1-43`
D29 — `src/renderer/components/dialogs/DebugConsoleDialog.tsx:1-17,595-600`
D30 — `src/renderer/components/dialogs/LogsDialog.tsx:1-20`；`src/renderer/components/panels/LogPanel.tsx`
D31 — `src/renderer/components/dialogs/CopySelectorDialog.tsx:1-43,101-110`
D32 — `src/renderer/components/dialogs/ContextReportDialog.tsx:12-20`
D33 — `src/renderer/components/dialogs/ActiveToolsDialog.tsx:12-17,64-70`
D34 — `src/renderer/components/dialogs/JobsDialog.tsx:22-29,138-145`；`src/renderer/components/dialogs/JobWidgetCard.tsx`
D35 — `src/renderer/components/dialogs/ChangelogDialog.tsx:1-22`
D36 — `src/renderer/components/dialogs/ForceToolDialog.tsx:1-10,119-125`
D37 — `src/renderer/components/dialogs/HotkeysDialog.tsx:120-140`；`src/renderer/lib/keymap.ts:209-353`
D38 — `src/renderer/components/dialogs/FirstRunOnboardingDialog.tsx:1-60,139-145`
D39 — `src/renderer/components/dialogs/ThemePickerDialog.tsx:1-15,178-186`
D40 — `src/renderer/components/dialogs/PlanApprovalDialog.tsx:1-23,185-195`；`src/renderer/stores/plan-approval.ts`
D41 — `src/renderer/components/dialogs/ExtensionDialog.tsx:1-12,239-260,640-660`；`src/renderer/components/dialogs/ApprovalDialog.tsx:1-30`
D42 — `src/renderer/components/dialogs/ConversationSearchDialog.tsx:1-10`
D43 — `src/renderer/components/dialogs/FeedbackDialog.tsx:1-10,81-90`
D44 — `src/renderer/components/dialogs/BenchmarkDialog.tsx:1-33,84-90`
D45 — `src/renderer/components/tasks/TaskEditorDialog.tsx:1-40`；`src/renderer/components/tasks/TaskForm.tsx`
D46 — `src/renderer/components/settings/ProviderConfigDialog.tsx:1-40,814-935`
D47 — `src/renderer/components/panels/pr/PrCreateDialog.tsx:1-25,89-95`
D48 — `src/renderer/components/panels/mcp/McpServerWizard.tsx:20-40,177-183`
D49 — `src/renderer/components/common/ConfirmDialog.tsx:1-46`
D50 — `src/renderer/components/common/Toast.tsx:1-53`
D51 — `src/renderer/components/layout/HistorySearchOverlay.tsx:1-63`
D52 — `src/renderer/components/chat/TranscriptFindBar.tsx:1-45`；`src/renderer/components/chat/ChatStream.tsx:669-760`
D53 — `src/renderer/components/layout/TabSearchMenu.tsx:1-43`
D54 — `src/renderer/components/agents/AgentHoverCard.tsx:1-63`；`src/renderer/App.tsx`（`<AgentHoverCardLayer/>`）
