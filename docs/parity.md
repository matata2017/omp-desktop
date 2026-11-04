# 复刻对照表（parity）

上游：`nornzach/oh-my-pi-gui` v0.9.35（MIT）。本文件是 `replica/features.csv` 的活账本：
功能条目来自侦察，**状态与证据由本工程维护**。

状态词：`todo` 未开始 / `wip` 进行中 / `done` 已实现且有验证 / `degraded` 降级（说明原因）/ `skip` 不复刻。

机器可读的源是 `replica/features.csv`（133 行，priority must 23 / should 89 / could 21；其中 33 条 surface 级增补集中在文末「增补」一节）。
本表按 area 汇总，逐条状态写在下面的清单里。

## 汇总

| area | 条目 | must | 当前状态 |
|---|---|---|---|
| agents | 7 | 0 | 见下 |
| browser | 4 | 0 | 见下 |
| chat | 20 | 8 | 见下 |
| files | 5 | 0 | 见下 |
| i18n | 2 | 0 | 见下 |
| memory | 1 | 0 | 见文末增补 |
| models | 7 | 3 | 见下 |
| packaging | 4 | 2 | 见下 |
| panels | 6 | 0 | 见下 |
| pr | 2 | 0 | 见文末增补 |
| scheduler | 5 | 0 | 见下 |
| sessions | 15 | 3 | 见下 |
| settings | 11 | 0 | 见下 |
| stats | 5 | 0 | 见下 |
| system | 13 | 1 | 见下 |
| tabs | 7 | 1 | 见下 |
| terminal | 4 | 0 | 见下 |
| themes | 3 | 0 | 见下 |
| tools | 12 | 5 | 见下 |

计数含 33 条 surface 级增补；这些行的逐条状态统一列在文末「增补」一节，均为 `todo`。

## 已知偏差（与上游行为不同，需持续维护）

| 面 | 偏差 | 原因 | 影响 |
|---|---|---|---|
| `src/renderer/main.tsx` | 增加 `installOmpApi()` 一行 | Tauri 侧没有 preload，`window.omp` 由 `src/bridge/omp-api.ts` 注入 | 其余 renderer 文件除下表四项外字节级一致 |
| `src/renderer/index.html` | CSP 增加 `ipc:`/`http://ipc.localhost`/`ws://localhost:*`；入口改 `/main.tsx`；`pre-paint.js` 改绝对路径 | Vite 与 Tauri 的 IPC 需要 | 无 |
| `src/renderer/components/chat/ChatStream.tsx` | `awaitingTranscript` 增加“status ready 且 tabHasHistory 且 rows===0 且未流式”分支 | sidecar ready 早于首屏 `get_messages`，原逻辑会先渲染空会话页再显示历史（点击会话的三段式闪烁） | 变成“启动/加载骨架 → 内容”，不再闪“想做点什么?”；上游文件不再字节级一致 |
| `src/renderer/hooks/use-session-switch.ts` | `canBrowseInPlace` 允许任何空闲、同 workspace/kind 的 tab 原地 `switch_session`（不再要求 blank/browsing） | Codex 式的常驻 agent 线程切换：点击侧栏不应每次新起 244MB sidecar | 同 workspace 点击不再新开 tab/进程；跨 workspace 仍新开；流式/草稿/审批期间不切换 |
| `src/renderer/lib/markdown-links.test.tsx` | 从 tsc/vitest 排除 | 该测试 import main 进程的 `toAbsolutePath`，而 main 进程已由 Rust 接管 | 该用例暂不参与回归，Phase 4 用 Rust 侧等价实现补回 |
| `linkedom` | 保留为 devDependency | 30+ 个 renderer 测试用它建 DOM | 与计划"删除 linkedom"相反；删了测试跑不了 |
| 内置浏览器 | Phase 8 前一律返回 `unknown channel: browser:*` | 子 webview + CDP 未实现 | Browser 面板在 UI 上会退化为错误提示 |
| `models.yml` 写入 | Rust 侧用 `serde_yaml` 全量重写，**用户手写的注释与键序不保留** | 上游用 `parseDocument`/`setIn` 做文档级合并；Rust 侧没有等价的注释保留 YAML 编辑器 | GUI 一旦保存 provider，文件里的注释会消失；键序仍稳定（映射插入序）。字段级未知键已保留（有单测） |
| `editor:open-external` | 用 Rust 进程跑 `$VISUAL`/`$EDITOR`（Windows 回退 `notepad`），非零退出返回 `text:null` | 上游 `src/main/editor.ts` 同流程，但其 `resolveEditorCommand` 还会读登录 shell 的 rc 文件 | GUI 进程 env 里没有 `$EDITOR` 时一律走 notepad；登录 shell 里的编辑器设置不生效 |

## 契约门

| 门 | 测试 | 状态 |
|---|---|---|
| argv 与能力矩阵一致 | `crates/omp-agent/tests/argv_caps.rs`（5 条：fork 全能力、stock 全降级、单项能力独立门控、用户 flag 不可覆盖 GUI 控制项、scheduled run 额外 denylist） | 通过：`cargo test -p omp-agent --test argv_caps` → 5 passed |
| stats 路径与 vendor 一致 | 待 `vendor/stats-server.ts` 就位 | 未开始 |
| 协议类型与 vendor 一致 | 待 `vendor/rpc-types.ts` 就位 | 未开始 |

## 逐条清单

### agents

| 功能 | 优先级 | 载体 | 状态 |
|---|---|---|---|
| Agent Hub「定义」页：发现 project/user/bundled agent，启用/禁用，按 agent 覆盖模型与 prewalk | should | reuse | todo |
| Agent Hub「Hub」页：实时 subagent 表（状态/耗时/模型）、transcript 抽屉、单 agent abort/revive | should | reuse | todo |
| 侧栏 agent 行：会话下挂当轮 agent、活动文案与计数 pill，历史 transcript 折叠 | should | reuse | todo |
| Collab relay：host / join / leave 共享会话，带只读视图 | could | reuse | todo |

### browser

| 功能 | 优先级 | 载体 | 状态 |
|---|---|---|---|
| Browser 面板：地址栏、多页 chip、前进/后退/刷新/停止、DevTools 入口 | should | reuse | todo |
| Agent 驱动页面：本地 WebSocket relay + 每页 CDP attach/send/createTab + tabSnapshot | should | rust | todo |
| 持久登录与页面休眠：`persist:omp-browser` 分区、离屏页面停帧、agent 活跃期唤醒 | should | rust | todo |
| 回复内链接路由：普通点击进面板浏览器，⌘/Ctrl-click 与 chat tab 走系统浏览器 | should | reuse | todo |

### chat

| 功能 | 优先级 | 载体 | 状态 |
|---|---|---|---|
| Prompt 发送与流式增量渲染（`text_delta`，节流写入、只重解析尾部） | must | reuse | todo |
| 流中 steer 与 follow-up 队列（Enter steer / ⌘Enter follow-up，按住修饰键预览） | must | reuse | todo |
| Abort 当前轮与 abort-and-prompt | must | reuse | todo |
| Markdown 渲染：GFM + 原样 HTML 白名单 sanitize + 本地/网页链接解析 | must | reuse | todo |
| 代码块：highlight.js 懒加载、字符上限、行号、复制 | must | reuse | todo |
| 工具调用卡：实时 running 卡、结果回填、过程分组折叠 | must | reuse | todo |
| 虚拟化 transcript：稳定行 key、贴底跟随、滚动位置恢复、游标分页 | must | reuse | todo |
| 扩展 UI 与工具审批对话框：select/confirm/input/editor/open_url、超时倒计时、审批样式 | must | reuse | wip（sidecar 请求帧不再被当响应丢弃；回复按来源 tab 回送 `extension-ui:respond`/`host-tool:*`/`host-uri:result`；需扩展实测） |
| 思考块：折叠、实时时长、level 标注 | should | reuse | todo |
| Markdown 增强：KaTeX 公式（`$…$` / `$$…$$`、美元价格守卫）与 Mermaid 图（懒加载、主题、SVG 消毒、失败回退源码） | should | reuse | todo |
| 图片附件：粘贴/拖拽图片转 ImageContent 并在 composer 预览 | should | reuse | todo |
| @file 提及与 /slash 补全：按 cwd 缓存文件列表、命令参数提示、输入历史 | should | reuse | todo |
| Plan 审批：批准并可执行/压缩上下文/保留上下文/保存，附反馈 | should | reuse | todo |
| transcript 内查找：⌘F 打开、命中高亮、上一个/下一个、计数 | should | reuse | todo |
| Composer 上方 dock 卡：Todos（拖拽/内联编辑/`set_todos`）、Plan、Goal、Agents、Queue | should | reuse | todo |
| 用量与轮次行：tokens/cost/cache/tps、turn 状态与失败原因 | could | reuse | todo |
| 语音：听写（边说边上屏、按短语落到光标）与实时语音会话 | could | reuse | todo |

### files

| 功能 | 优先级 | 载体 | 状态 |
|---|---|---|---|
| 工作区文件树：`fs:list` 深度默认 8（上限 16）、总文件数 2000（上限 20000）、跳过 `.git`/`node_modules` 等 | should | rust | todo |
| 文件预览与 @mention 插入：markdown/代码渲染、默认 200 KB 截断（上限 2 MB）、按行定位、图片 data URL | should | hybrid | todo |
| 打开/在文件管理器中显示/系统编辑器，路径右键菜单 | should | plugin | wip（`system:open-path`/`reveal-path`/`resolve-paths` 已通；系统编辑器与右键菜单未做） |
| Git 变更视图：仓库状态、逐文件 diff、working tree 自动刷新 | should | hybrid | todo |
| 多根工作区目录：列/加/删目录并重置会话工作区 | should | rust | todo |

### i18n

| 功能 | 优先级 | 载体 | 状态 |
|---|---|---|---|
| 三语界面（en / zh / ru）+ 未知 key 回退英文 | should | reuse | todo |
| 全局语言切换：跨窗口广播、原生菜单/托盘/对话框文案、日期格式化 locale | should | hybrid | todo |

### models

| 功能 | 优先级 | 载体 | 状态 |
|---|---|---|---|
| 模型选择器：按 provider 分组、搜索、目录增量刷新 | must | reuse | wip：`get_providers`(89)/`get_available_models`(8) 与模型指示器已通（CDP 实测 composer 显示 `group/never-stuck · 超高 · 快速`），分组/搜索 UI 未逐项验 |
| 模型与推理控制：`set_model` / `cycle_model`（⌃P 前后）/ thinking level / fast / slow | must | reuse | todo |
| Provider 管理：认证状态、OAuth / API key 登录登出、多账号选择 | must | reuse | todo |
| 自定义 provider：`models.yml` 保序原子读写、模型编辑器（context/limits/cost/thinking）、删除 | should | hybrid | done：`src-tauri/src/models.rs`（list/upsert/delete/ensure，apiKey 掩码尾 4 位、未知字段保留、内置 id 拒改、tmp+rename 原子写、7 单测）。CDP 实测 `models.listProviders()` 返回 1 条、首启阻塞弹窗与“无法读取自定义模型文件”黄条同时消失（`cad8df1`）；注释保留见已知偏差 |
| 模型角色分配：role → model + thinking，按 metadata 校验 | should | reuse | todo |
| Provider 用量窗口：配额窗口条 + 本地 token/cost 统计 + reset credit | should | reuse | todo |

### packaging

| 功能 | 优先级 | 载体 | 状态 |
|---|---|---|---|
| 内置 sidecar 解析：`OMP_BUNDLED_OMP` → `resources/omp(.exe)`，绝不回退系统 omp | must | rust | todo |
| sidecar 构建：`compileCodingAgent` 单文件、natives 目标架构暂存、stats/collab 资源生成 | must | rust | todo |
| 图标与平台元数据：icns/ico/png 集合、NSIS 允许改安装目录、Linux `executableName` | should | rust | todo |
| 安装包目标与 extraResources：dmg/zip、NSIS/portable、AppImage/deb + `omp://` 协议声明 | should | rust | todo |

### panels

| 功能 | 优先级 | 载体 | 状态 |
|---|---|---|---|
| 工作区抽屉：7 个视图（overview/agents/btw/diff/files/terminal/browser）、宽度拖拽、chat tab 只留 overview/btw/files | should | reuse | todo |
| Overview 面板：上下文分类占用、工作区、改动文件、运行中 subagent、MCP 服务器 | should | reuse | todo |
| Queue 面板：steering / followUp 两 lane 的编辑、移动、清空、恢复 | should | reuse | todo |
| 上下文与用量指示：标题栏 chip、popover、分类 breakdown、compact 策略条 | should | reuse | todo |
| MCP 管理面板与向导：添加/测试/重连/重授权/启停/删除 | should | reuse | todo |

### scheduler

| 功能 | 优先级 | 载体 | 状态 |
|---|---|---|---|
| 定时任务 CRUD：once/cron/interval 编辑器、深度与工具策略、草稿校验、修订历史 | should | rust | wip：`crates/omp-scheduler` 四表 schema + user_version 迁移、CRUD 与修订链、5 段 cron（vixie day OR、DST、拒绝 6 段）、23 单测；`scheduler:get-snapshot/create/update/set-enabled/archive/delete/task-runs/revisions/set-paused` 已接。经 bridge 实测建任务→列表 1 条(tools=read_only,rev=1)→update rev=2 且标题生效。validate 仍返回空 problems |
| 后台运行：独立 headless sidecar、read_only / worktree 策略、超时、报告采集、worktree 提交或清理 | should | rust | wip：运行 argv 由 `src-tauri/src/scheduler.rs` 代码控制——审批模式跟随工具策略（read_only→always-ask+六个只读工具、workspace→write、full→yolo），并从用户 flag 中剥掉 `--approval-mode/--yolo/--auto-approve/--plan-yolo/--tools/--no-tools/--model/--continue`（单测：yolo profile 塞不进去）；run-now 起独立 sidecar + 独立 session-dir + 发 prompt。**未跑真实运行**（耗额度）；超时/worktree/报告采集未接 |
| 运行历史与详情：state / 耗时 / usage / changes / verify 徽章与逐次回放 | should | reuse | wip：`scheduler:task-runs`/`get-run` 从库读记录；UI 未逐项验 |
| Review 队列：accept / reject + note | should | reuse | todo |

### sessions

| 功能 | 优先级 | 载体 | 状态 |
|---|---|---|---|
| 会话索引：256 B 标题槽 + 32 KiB 头/尾解析、mtime 倒序、状态推断、4096 条 LRU | must | rust | wip（`crates/omp-session` 改为 32 KiB 头 + 32 KiB 尾窗口解析、标题/头行/首条用户消息/尾计数与状态、按 (size,mtime) 缓存 4096 条；侧栏列表明细实测 ~90ms） |
| 侧栏会话列表：code/work 双模式、按 cwd 分组、pinned/archived、family 折叠 | must | reuse | wip（workspace 分组、family 折叠已通：`SessionInfo.parentSessionPath` 已从 header `parentSession` 解析；pinned/archived 未接） |
| 新建/切换会话：busy 守卫、会话归属去重（已打开则聚焦 owner） | must | hybrid | wip（同 workspace 空闲 tab 原地 `switch_session`，不再新开进程；只有 `success && cancelled:false` 才更新归属，`cancelled:true` 是 hook veto/跨 workspace；流式/草稿/审批/队列守卫已测；首屏 `get_transcript_page` 在会话加载后约 10ms，主要成本是 switch 的 session 加载） |
| 会话重命名/删除/归档（含批量与运行中跳过） | should | rust | wip（delete/rename 已 CDP 实测：删除清文件+子目录、越界路径被拒；重命名改写标题槽 `source:user`；archive/batch/运行中跳过未做） |
| 会话内容搜索：`sessions:search` 多 token 全命中、`mtime:size` 缓存 | should | rust | wip（多 token 命中已通；改为按 (size,mtime) 缓存搜索文本、单文件 8 MiB 上限、总 64 MiB 预算） |
| 跨会话消息搜索：SQLite 索引 + 命中片段 + 打开到指定 entryId | should | rust | done：`crates/omp-session/src/conversation.rs`（FTS5 trigram、`mtime_nanos:size` 签名、追加式增量 + 64B 尾部校验、terms 全 AND、400 行/每会话 3 命中/60 会话、片段用 U+0001/U+0002 标记、库打不开返回 `unavailable`）+ `conversations:search` 通道（懒开索引、`spawn_blocking`）。实测 `indexed:5`，查「复刻」返回 3 个会话带片段；6 个单测 |
| 会话树：分层树布局、平移/缩放、过滤模式、标签、切换 leaf、fork 到新窗口 | should | reuse | todo |
| 从消息 branch / fork / fork_from | should | reuse | todo |
| 导出 HTML：保存对话框 → `export_html` → toast | should | reuse | todo |

### settings

| 功能 | 优先级 | 载体 | 状态 |
|---|---|---|---|
| Schema 驱动设置窗口：tabs/groups/controls 来自 `get_settings_schema`，写入 `set_setting` 即时生效 | should | reuse | done：`get_settings_schema` 返回 242 KB schema，设置窗口按它渲染出 8 组（外观与使用/模型与账号/任务与上下文/工具与执行/扩展与资源/权限与安全/记忆/系统与高级）+ 开关与“选择 GUI 主题”（截图在 `.probe/romp-settings.png`）；`set_setting` 走通用 RPC，未逐项点 |
| GUI 偏好：主题、密度、字号、通知、tab 宽度/行数、默认面板、面板宽度 | should | hybrid | wip（`prefs:get/set` + `prefs:update-launch-profile` 已通，落 `prefs.json`；其余偏好项未接） |
| 审批模式：always-ask / always-ask-cached / write / yolo，随 spawn argv 生效 | should | hybrid | todo |
| 运行时开关：steering / follow-up / interrupt 模式、auto-compaction、auto-retry | should | reuse | wip：`get_state` 回传这些字段（`steeringMode`/`followUpMode`/`interruptMode`/`autoCompactionEnabled`/`autoRetryEnabled`），RPC 通用转发已通；未逐项切换验证 |
| 管理页：Security 扫描、Skills 启用/编辑、SSH host 增删改测、Updates 版本与检查 | could | reuse | todo |

### stats

| 功能 | 优先级 | 载体 | 状态 |
|---|---|---|---|
| 统计仪表盘外壳：10 个路由、1h/24h/7d/30d/90d/all、手动 sync、打开时静默同步 | should | reuse | done（后端）：`stats:url` 在服务起来后返回 `http://127.0.0.1:61209/`，`stats:fetch` 转发到白名单路由；路由/切换时间窗的 UI 未逐项点 |
| Overview 路由 | should | reuse | done（数据面，逐窗口对账通过）：`/api/stats/overview` 的 `overall` 与 `stats.db` 在**同一窗口**（`firstTimestamp..lastTimestamp`）完全一致——`totalRequests=32` vs `messages` 32 行，`totalInputTokens=2782801`、`totalOutputTokens=40291` 三项逐字节相同。注意 `message_rollup.bucket` 是**小时桶**，无法用于亚小时窗口对账，必须用 `messages`。渲染层的时间窗切换未逐项点 |
| Models 路由（模型维度 token/cost/速度） | should | reuse | done（数据面）：`/api/stats/models` 返回 `workbuddy/step-5-preview`、`group/never-stuck`，与 `stats.db` 的 `select distinct model` 完全一致 |
| 统计服务托管：`omp stats --host 127.0.0.1 --port 0 --no-open`、ANSI 剥壳取端口、指数退避重启 | should | rust | done：`crates/omp-stats`——**setup 时后台启动**（蓝图要求 app start 就起，不是首次 fetch），15s 等端口，ANSI 剥壳解析，`RestartBudget` 成功即清零（连签失败才耗尽），`ensure_running` 每次校验子进程存活。实测：启动日志 `stats dashboard ready port=62574`；`Stop-Process` 杀掉 stats 子进程后再次 `stats:fetch` **329ms 内换端口 62670 自愈**，新子进程 pid 16448；非法路径本地拒绝；8 个单测 |
| 其余 8 条路由：costs / errors / requests / providers / tools / frustration / gain / projects | could | reuse | wip：转发层已通（`/api/stats/requests` 返回分页对象 `{rows,total,nextCursor}`），逐路由渲染未验 |
| Benchmark：`omp bench --json` 子进程、1–8 模型、profile/runs/par ≤ 20/8、15 分钟超时、abort 即取消 | could | rust | done：`src-tauri/src/bench.rs`（argv 校验逐条对齐 `benchmark-runner.ts`，stdout/stderr 各 8 MiB 上限，100ms 轮询 + 超时自杀，abort 清空子进程句柄让在跑的那次返回 `Benchmark aborted`）；4 个单测。CDP 实测（全部为校验分支，**未起子进程**，`Get-CimInstance` 查 `omp.exe * bench *` 计数为 0）：空模型 / 9 个模型 → `Select between 1 and 8 models`；`--evil` → `Invalid model selector`；profile `throughput` → `Invalid benchmark profile`；runs 99 → `runs must be between 1 and 20`；无运行中时 `bench:abort` → `false`。**真实 benchmark 未跑过**（最长 15 分钟 × 模型额度），需要时由用户点头再跑 |

### system

| 功能 | 优先级 | 载体 | 状态 |
|---|---|---|---|
| 原生应用菜单：File/Edit/View/Window/Help，动作经 `MENU_ACTION` 回渲染层；renderer 拥有的 chord 不设 accelerator | should | rust | todo |
| `omp://` 深链 + 单实例锁：`new-session` / `switch-session` | should | plugin | todo |
| 窗口几何持久化：多显示器可达性修正、多窗口级联、最大化状态 | should | rust | todo |
| 退出守卫：有运行中会话/定时任务时确认；`app:quit` 是唯一退出路径 | should | rust | todo |
| 系统集成：桌面通知、打开/保存对话框、剪贴板读取、openExternal、录制文件落盘 | must | plugin | wip（4/5 已验证）：`system:notify` / `system:clipboard-read` / `system:open-external` / `recorder:save` 已接并实测（CDP：clipboardRead 260 字符、notify ok、recorderSave 写入 `[1,2,3,250]` 校验通过，字节数组与 base64 两种载荷都有单测）；`system:save-dialog` / `system:open-dialog` 已接（blocking 变体跑在 spawn_blocking，取消返回 null），但需要人工点一次原生选择器才能确认 |
| 命令面板：⌘K 搜索、分类、子菜单、prompt 透传、TUI-only 标记不可用 | should | reuse | done：`get_available_commands` 返回 87 KB 命令表并经 `commands:update` 推到渲染层；实测点开命令面板列出真实条目（工作区/添加目录、Git 仓库变更、移动会话、提供商登录/登出…） |
| 自动更新：4h 轮询、manual/automatic 两模式、下载/应用、GitHub releases 源 | could | plugin | todo |

### tabs

| 功能 | 优先级 | 载体 | 状态 |
|---|---|---|---|
| 多标签：每 tab 独立 sidecar / session / 事件路由，上限与 warm spare，未显示不 spawn | must | hybrid | wip（`tab:spawn/close`、per-tab client/batcher 已通；新增 warm spare（启动后预热，spawn 直接 adopt + `switch_session`）、默认上限 3（`ROMP_MAX_SIDECARS` 可调）、LRU 回收可 resume 的空闲 tab；session 归属/布局持久化未做） |
| Tab 类型 agent / chat：chat 无工具、历史与项目组分离 | should | hybrid | wip（`tab:spawn` 传 kind，chat 走 `--chat`；历史分离未验证） |
| Tab 条：拖拽排序、状态 chip、运行中关闭内联确认、右键菜单、⌘1–9 / ⌃Tab | should | reuse | wip（renderer 原样；`setOrder` 仅回 true，未持久化） |
| 双栏拆分：左右/上下、拖拽 ratio、键盘调整、拖 tab 到边缘成对 | should | reuse | wip（`setView` 已接 pool；groups/ratio 持久化未做） |
| Tab 布局持久化与延迟启动恢复：tabs + activeIndex + groups | should | rust | done：`src-tauri/src/tab_layout.rs` 落 `<app_data>/tab-layout.json`（原子写；加载丢弃 workspace 已不存在或 session 已删的 tab，activeIndex 越界归零）；`tab:spawn/close/set-active/set-view/set-order` 后持久化；启动按布局注册全部 tab、只启 active 的。实测开 2 个 tab 重启后仍 2 个、activeIndex 一致。注意 sidecar 退出会删除无消息的空会话，这类 tab 恢复成同 workspace 的新会话；groups 分屏比例未存 |
| 多窗口：每窗口独立 tabs，会话可"在新窗口打开"，重复打开聚焦 owner | should | rust | todo |
| Worktree tab：创建/删除 worktree、tab cwd 绑定 worktree、关闭时提示 | could | hybrid | todo |

### terminal

| 功能 | 优先级 | 载体 | 状态 |
|---|---|---|---|
| 终端面板：每 tab 多 shell、xterm + fit、颜色随 GUI 主题 | should | reuse | wip（`terminal_open` 实测返回 terminalId/pid；`terminal_opened` + `terminal_output` 以 `{tabId,frame}` 到达 renderer，data/seq 正确；xterm 面板 UI 待人工确认） |
| Agent `pty:true` bash 接入终端并自动 reveal | should | reuse | wip（`terminal_opened` 已路由，renderer 的 reveal 逻辑原样；待真实 agent bash 验证） |
| 终端生命周期同步：open/input/resize/close/list/snapshot，快照与后续帧用 seq 对齐 | should | hybrid | wip（RPC 命令走 sidecar，帧路由已补齐；seq 对齐由 renderer 原样处理） |
| 终端链接与 CLI 转发：识别路径/URL，`terminal_run_cli` / `tui_open` | could | reuse | todo |

### themes

| 功能 | 优先级 | 载体 | 状态 |
|---|---|---|---|
| 内置主题：12 套命名主题 + system 跟随系统，119 个 `--omp-*` token 全量写入 | should | reuse | todo |
| 自定义主题目录：`~/.omp/gui/themes/*.json`，extends/tokens 校验、热更新、错误提示 | could | hybrid | wip（`themes:list`/`themes:open-folder` 已通，含解析错误字段；热更新事件未做） |
| Agent/插件主题叠加与首帧引导：follow agent theme、终端调色板、插件 token overlay、pre-paint | could | reuse | todo |

### tools

| 功能 | 优先级 | 载体 | 状态 |
|---|---|---|---|
| read 卡：文件预览、语法高亮、行号、重复 read 折叠成组 | must | reuse | todo |
| write 卡：路径 + 内容预览、可折叠、成功/失败态 | must | reuse | todo |
| edit / apply_patch / ast_edit 卡：diff 渲染（复用 `lib/diff` 的 parseDiff/DiffView）、流式参数稳定 | must | reuse | todo |
| bash 卡：命令 + 流式输出 + exit code + 输出压缩提示（Minimized: a → b） | must | reuse | todo |
| 未知 / MCP / host 工具通用卡：名称、摘要、JSON 参数与结果 | must | reuse | todo |
| grep / glob / find 结果卡：文件:行:内容、命中高亮、文件树式 glob | should | reuse | todo |
| task 卡：子代理 spawn 进度、日志与结果 | should | reuse | todo |
| web_search 卡：结果列表、来源链接、查询回显 | should | reuse | todo |
| 后台协调卡：`wait` job、`proc://` 任务/服务、`agent://` 消息与 hub 进程表 | should | reuse | todo |

## 增补（surface-level 功能点，全部 todo）

由 `replica/parts/screens.md` 的 76 个 S/D 表面反查补齐；主清单的 100 行未覆盖这些整块表面。逐条证据见 `replica/parts/features.md` 与 `replica/parts/screens.md`。

| 功能 | area | 优先级 | 载体 | 状态 |
|---|---|---|---|---|
| Agents 面板（roster）：turn/all 范围、list/DAG 双视图、暂停/停止/展开、失败未读点 | agents | should | reuse | todo |
| 子代理页：顶替对话 / 并排两种布局、只读 transcript、stop/revive、面包屑回会话 | agents | should | reuse | todo |
| 子代理悬浮卡：400ms 延迟预览状态、now-line、最近 3 步，键盘聚焦同效 | agents | could | reuse | todo |
| BTW 旁问：面板 + 对话框、线程 running/complete/cancelled/error/interrupted、分支回主会话 | chat | should | reuse | todo |
| 全屏 composer 编辑器：CodeMirror markdown、外部编辑器往返（$VISUAL/$EDITOR）、未保存丢弃保护 | chat | should | reuse | todo |
| ask 工具卡：多问题单选/多选/自定义答案，与扩展 UI 队列共用超时与取消 | chat | should | reuse | todo |
| 会话信息对话框：getSessionStats 的身份、消息/工具计数、token 拆分、premium 请求、成本、上下文 | sessions | could | reuse | todo |
| Handoff：让 agent 总结当前会话并在新会话继续，保留原会话 | sessions | should | reuse | todo |
| 分享与导入会话：preview_share_session / share_session 上传链接；Claude/Codex 会话导入为新 OMP 会话 | sessions | could | reuse | todo |
| 工作区切换器：列已知工作区、跳转或新增目录（原生目录选择） | sessions | should | hybrid | todo |
| 模型恢复：会话保存的模型不可用时，选择用当前模型打开或留在原地 | sessions | could | reuse | todo |
| Diff 面板四 scope：repository/current/timeline/artifacts，unified/split、多文件选择、单文件错误隔离 | panels | should | reuse | todo |
| PR Center：PR 列表 + CI 徽标 + diff 统计、详情 markdown、按文件懒加载 diff、checkout 到 worktree | pr | should | hybrid | todo |
| AI 起草创建 PR：标题/正文草稿、base/head 选择、提交后刷新列表 | pr | should | hybrid | todo |
| Memory 窗口：knowledge/skills/sources 三视图、项目选择、纠正/遗忘、自学技能待复习 | memory | should | reuse | todo |
| Modes 窗口：Vibe / Goal / Loop 三种会话模式 | settings | should | reuse | todo |
| Extensions 窗口：hooks / MCP / commands 三 tab、行内开关乐观更新失败回滚、MCP 动作菜单 | settings | should | reuse | todo |
| Inventory 窗口：plugins / marketplaces / prompt templates / memory 报告，插件详情抽屉 | settings | could | reuse | todo |
| Model compare：全模型矩阵（auth、上下文、$/1M、配额、角色、当前模型），排序过滤与行内 set_model | settings | should | reuse | todo |
| 快捷键参考 / 改键：可搜索、逐行录制 chord、冲突阻断或警告 | settings | should | reuse | todo |
| 首次运行引导与设置产品页：无可用 provider 时引导登录/自定义 YAML；Capabilities/Advanced 页 | settings | could | reuse | todo |
| 系统托盘：配额环图标、聚合运行状态、quick config（fast/thinking/approval/language）、快捷动作与 show/hide + quit | system | should | rust | todo |
| 诊断面：Debug 控制台（debug RPC）、应用日志（runtime log 追尾）、变更日志 | system | could | hybrid | todo |
| 会话内工具面板：/tools 活动工具、/jobs 异步任务、/force 强制工具、复制目标选择器 | system | should | reuse | todo |
| 反馈对话框：GitHub issue 预填（标题/正文/labels）+ 诊断信息预览 | system | could | reuse | todo |
| 模型 benchmark：profile / runs / 并行度、结果矩阵、可取消 | system | could | hybrid | todo |
| sidecar 状态横幅：restarting/error/exited（含 ready 但 sidecarError）+ 重启入口 | system | should | hybrid | todo |
| 通知铃：未读定时任务运行、按运行打开、全部已读 | scheduler | should | reuse | todo |
| 开发/运维工具卡：eval、debug、lsp、ast_grep、resolve/reject、checkpoint/rewind、context_notes/new_context | tools | should | reuse | todo |
| 协调/代理工具卡：todo/goal/schedule_task/manage_scheduled_tasks、hub/wait、browser/computer/github | tools | should | reuse | todo |
| 记忆/技能/安全/生成工具卡：retain/recall/reflect/memory_edit、learn/manage_skill、security_scan/ida、vibe_*、generate_image | tools | could | reuse | todo |
| 会话选择器（resume）：本目录/所有项目 scope、字面+模糊+全文检索、排序与路径显示 | sessions | should | reuse | todo |
| 侧栏配额摘要：provider 配额窗口条 + 最紧窗口，与 Usage 窗口同源 | models | could | reuse | todo |
