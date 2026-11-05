# 交接文档（handoff）

最后更新：2026-11-05。目标：把 `nornzach/oh-my-pi-gui`（Electron 35 + React 19，v0.9.35，MIT）
的功能复刻成 Rust/Tauri 2 桌面应用 `Romp`（内部 crate 仍用 `omp-*` 指代上游协议），常驻内存与冷启优于 Electron 版。

权威输入有四份，冲突时按此顺序：

1. `replica/recon.md` + `replica/parts/*`（**建什么**：22 屏 / 54 对话框 / 26 流程 / 28 实体 / 133 条功能，逐条带 `文件:行` 证据）
2. `docs/parity.md`（**做到哪**：逐条状态账本，机器可读源 `replica/features.csv`）
3. 已批准的实施计划（**怎么建**：Phase 0–9 的步骤与验收）
4. `docs/architecture.md`（**架构怎么选**：Rust 常驻服务 + 可回收 agent；内存/启动预算与迁移顺序）

---

## 1. 当前状态

| Phase | 内容 | 状态 |
|---|---|---|
| 0 | 工具链与镜像 | **完成**：Rust 1.99.1 + MSVC 14.44.35207 + Win11 SDK 26100 + bun 依赖；`scripts/dev.ps1` 可起窗 |
| 0.5 | LSP + 调试器 | 配置完成，调试会话未验证 |
| 1 | Rust workspace + RPC 传输 + sidecar | **完成**（6 条协议探针全 success，证明是 fork 版） |
| 2 | Tauri 外壳 + `window.omp` shim | **完成**：起窗、ready、会话列表、完整 prompt→流式→工具卡→agent_end；终端帧路由实测 |
| 3 | 会话 / 多标签 | **2/5**：原地 `switch_session`、会话增删改查、per-tab client/batcher、LRU 回收、**布局持久化 + 重启恢复**已完成；剩 phantom tab 定位、租约矩阵、归档/树/branch-fork |
| 4 | 对话面与系统通道 | **4/5**：模型指示器、system 对话框/剪贴板/通知、`editor:open-external`、**会话搜索（FTS5）**已完成；剩会话树/分支/导出 |
| 5 | 模型 / Provider / 设置 | **3/4**：`models:*` 全套（含 models.yml 原子读写与内置 id 守卫）、schema 驱动设置页（读写都验过）、命令面板已完成；剩 provider 登录/登出 |
| 6 | 统计 | **完成**：`crates/omp-stats` 懒启动改 setup 预启动、17 路径白名单 + `/api/request/:id`、`bench:run/abort`、与 `stats.db` 同窗口对账通过 |
| 7 | 定时任务 | **4/4**：`crates/omp-scheduler` 四表 + 修订链 + 5 段 cron、`scheduler:*` 全通道、**运行 argv 由代码控制**、**监督循环到终态**、`schedule_task`/`manage_scheduled_tasks` host tools 已完成；含通知铃与 Insights（均由 `insights.rs` 派生，见 parity） |
| 8 | 终端与浏览器 | 0/4：终端面板视觉未确认；**browser webview host 未实现**（最大的剩余块） |
| 9 | 打包 / 深链 / 托盘 / 性能 | 1/3：性能已实测写进 `docs/perf.md`；NSIS 未构建，深链/托盘未接 |
| R | replica 侦察 | **完成** |

### 已验证的事实（不是推断）

```
bunx tsc --noEmit                                → 0 error
bunx vite build                                  → 成功，vendor 分块与上游一致
bun run test                                     → 186 files / 1571 passed
cargo test --workspace                           → 142 passed（16 suites，0 failed）
cargo run -p omp-agent --example probe -- resources/omp.exe
  ready: protocolVersion=1 v2=true
  get_state / get_available_models / get_settings_schema
  terminal_list / get_jobs / get_context_report → 全 success=true
bunx tauri dev                                   → Romp ready；prompt "list files in crates" → 流式回复 + 4 步工具卡 + agent_end
node scripts/cdp-eval.mjs                        → CDP eval/输入辅助（9222）
sidecar click 观测                                 → cold 短会话首条 bubble 929ms；warm 切换 653ms；spawn→ready 600–950ms
                                                   → ChatStream 加载门修复后，cold 打开不再闪“想做点什么?”空会话页
terminal_open 实测                                 → terminalId/pid 返回；terminal_opened + terminal_output 以 {tabId,frame} 到达 renderer
fresh tab argv                                     → --mode rpc-ui --no-auto-resume（fork 已探针验证）
switch_session 同 workspace                         → cancelled:false 且 sessionId 真的切换；跨 workspace 为 cancelled:true
sessions:list / delete / rename / search             → CDP 实测通过；索引改为 32 KiB 头尾窗口 + (size,mtime) 缓存
conversations:search                              → indexed:5；中文查询返回 3 会话；snippet 含 U+0001/U+0002 标记（CDP 验证，<mark> 高亮前提）
models:*                                          → 首启阻塞弹窗与“无法读取自定义模型文件”消失；listProviders 返回配置项
stats:url / stats:fetch                           → 服务 setup 即起（port=62574）；杀掉子进程后 329ms 自愈换端口
stats 对账                                         → 同窗口 requests=32 / in=2,782,801 / out=40,291 与 stats.db 完全一致
scheduler:*                                       → 建任务→列表(tools=read_only,rev=1)→update rev=2 且标题生效
scheduler run 终态                                 → runNow→cancelRun→记录 status=cancelled + endedAt（不再永远 running）
scheduler insights/notices                        → insights 12 字段齐（runs=1, median=2032ms）；notices() 返回数组；
                                                   review(run,'accepted') 写回后 reviewAcceptRate 由 null → 1
set_host_tools                                    → 用 parameters 字段返回 success:true（inputSchema 会被拒）
tab 布局恢复                                       → 开 2 tab 重启后仍 2 tab、activeIndex 一致；tabs 按数字后缀排序
app:quit                                          → 应用退出、D:\rust-omp 孤儿进程 0 个、runtime.log 无 panic
性能（dev 构建）                                    → 冷启 137–321ms；1 tab 883–894MB；4 tab 1273MB；每 tab ≈235MB
```

本机验证的关键启动修复：

- `ipc_invoke` 的 `payload` 改为 `Option<Value>`：`call()`/`fire()` 省略 payload 时 Tauri 不再拒绝参数。
- sidecar 改为 `tab:get-all` / `tab:set-active` 时懒启动；`sidecar:status-get` 回放当前状态，避免 ready 事件早于订阅。
- `runtime.log` 落盘到 `%APPDATA%\dev.romp.desktop\runtime.log`，`scripts/dev.ps1` 自动设置 MSVC/PATH。
- `ChatStream.tsx` 的 `awaitingTranscript` 增加“ready 且 tab 有历史且 rows===0”分支：sidecar ready 早于 `get_messages` 时继续显示骨架，避免三段式闪烁（已记入 parity 已知偏差）。
- **会话切换改为常驻 sidecar 原地切**（对标 Codex `app-server` 的 `thread/resume`）：同 workspace 的空闲 tab 直接
  `switch_session`，不再每次新起 244 MB 进程；跨 workspace、kind 不匹配、流式/草稿/审批/队列未清时仍新开 tab。
  配套：Rust 补发 `tab:status` 快照（上游 main 本来就发，缺它时 renderer 的 tab store 永远看不到 `ready`）。
- 终端帧路由：`terminal_opened/output/exit/closed` → `terminal:frame`，payload 用渲染层要求的 `{tabId, frame}`。

最后一条是 Phase 1 的硬门槛：`get_settings_schema` 与 `terminal_list` 在官方 stock 18.15.0 上返回
`Unknown command`，全 `success:true` 证明 `resources/omp.exe` 是 **fork 版 sidecar**（243.9 MB，
从 `omp-0.9.35-portable.exe` 的 NSIS 载荷 `$PLUGINSDIR/app-64.7z` 解出）。

---

## 2. 本轮（2026-11-05）新增能力一览

| 面 | 实现位置 | 验证方式 |
|---|---|---|
| 统计仪表盘 | `crates/omp-stats` + `stats:url`/`stats:fetch` | HTTP `/api/status` 200；与 `stats.db` 同窗口三项对账 |
| Benchmark | `src-tauri/src/bench.rs` + `bench:run`/`bench:abort` | 四条拒绝路径 + abort 语义（**未跑真实 bench**） |
| 定时任务存储 | `crates/omp-scheduler`（23 测试） | create→update rev 链→list 顺序、cron 边界 |
| 定时任务运行 | `src-tauri/src/scheduler.rs` + `scheduler:*` | runNow→cancelRun→`cancelled`+`endedAt` |
| 定时任务 host tools | `src-tauri/src/host_tools.rs` + `host_tools_defs.json` | `set_host_tools` 返回 `success:true` |
| 跨会话搜索 | `crates/omp-session/src/conversation.rs` | `indexed:5`，中文查询 3 会话，分隔符穿过 IPC |
| Tab 布局 | `src-tauri/src/tab_layout.rs` | 2 tab 重启恢复，activeIndex 一致 |
| models.yml 编辑 | `src-tauri/src/models.rs` | 首启阻塞弹窗消失；内置 id 拒改；原子写无残留 |
| 外部编辑器 | `src-tauri/src/editor.rs` | 3 单测（命令拆分、换行剥离、缺编辑器） |
| system 通道 | `ipc.rs`（dialog/clipboard/notification/recorder） | clipboard 260 字符、notify ok、recorderSave 字节一致 |
| argv 契约门 | `crates/omp-agent/tests/argv_caps.rs` | 5 条：三组能力矩阵 + 不可覆盖 + scheduled denylist |
| 性能采样 | `scripts/measure-perf.ps1` + `docs/perf.md` | 冷启 137–321 ms；1 tab 883–894 MB；4 tab 1273 MB |

---

## 3. 目录结构

```
crates/
  omp-core/     agent/data 路径、TabId/WindowId、SessionState 解析、Result 别名
  omp-rpc/      frame.rs（分块重组）client.rs（id 关联 + 超时表）transport.rs（NDJSON 泵）
  omp-agent/    argv.rs（能力门控 argv）sidecar.rs（进程/写通道/stderr）batcher.rs（32 ms 合批）
                pool.rs（tab→进程、deferStart、F-OWN 归属、F-UI-ORIGIN 回包路由）
  omp-rpc/examples/probe.rs    协议探针（Phase 1 验收）
  omp-session/  会话索引：32 KiB 头/尾窗口解析、标题/首条消息/计数/状态、(size,mtime) 缓存
                conversation.rs：FTS5 trigram 跨会话搜索（追加式增量 + 64B 尾部校验）
  omp-stats/    统计服务托管（`omp stats` 子进程 + ANSI 剥壳取端口 + RestartBudget + 存活校验）
                与白名单客户端（17 路径 + `/api/request/:id`，`/api/sync` 只 POST）
  omp-scheduler/ 定时任务存储：tasks/runs/revisions/meta 四表 + user_version 迁移 + 修订链
                cron.rs：5 段 cron 解析（vixie day OR、DST、拒绝 6 段）
scripts/         dev.ps1（注入 MSVC/PATH/OMP_SIDECAR + CDP 端口）、with-msvc.cmd、install-msvc.ps1
                 cdp-eval.mjs / cdp-cmd.mjs / measure-click.mjs（WebView2 9222 驱动）、capture-window.ps1
                 measure-perf.ps1（进程树 RSS 按角色拆分）、extract-host-tools.mjs（从上游 TS 生成工具 schema）
vitest.config.ts + scripts/test-locale.ts（上游缺，导致 268 个 locale 测试失败，已补）
src-tauri/      lib.rs（状态/事件路由/优雅退出）ipc.rs（唯一命令 ipc_invoke，全通道 match）
                models.rs（models.yml 读写）editor.rs（外部编辑器往返）bench.rs（`omp bench --json`）
                scheduler.rs（运行 argv 策略 + 监督循环）host_tools.rs + host_tools_defs.json（agent 的定时任务工具）
                tab_layout.rs（布局持久化）builtin_providers.rs（88 个内置 provider，生成）
src/renderer/   上游 v0.9.35 原样拷贝（567 文件 / 111,400 行），仅改 main.tsx、index.html、ChatStream.tsx（加载门）、use-session-switch.ts（原地切换）
src/shared/     上游类型与 rpc-client.ts 原样拷贝（合同的唯一来源）
src/bridge/omp-api.ts   Electron preload 的 Tauri 移植：OmpApi → ipc_invoke
replica/        侦察产物（recon.md、parts/*、features.csv）
docs/parity.md  复刻对照账本（10 done / 24 wip / 101 todo）
docs/perf.md    性能实测与口径（dev 构建，进程树口径）
docs/architecture.md  架构决策
.trellis/       Trellis 任务台账（01-tauri-shell 为本阶段任务）
resources/omp.exe  fork sidecar（gitignored，需按第 4 节重建）
```

---

## 4. 关键设计决策

1. **Rust 只做传输**。命令与帧一律 `serde_json::Value`；2,600 行 wire 类型不重写，命令构造与响应
   解码留在 TS（`src/shared/rpc-client.ts` 原样复用）。Rust 只在必须解释载荷处持有小结构
   （`get_state`、会话归属、请求 id 路由）。
2. **IPC 只注册一个命令**：`ipc_invoke(channel, payload)`，channel 字符串与 `IPC_COMMANDS` 完全一致，
   `src/shared/ipc-types.ts` 是两端唯一权威。
3. **事件必须发对形状**。渲染层有两种订阅：
   - `{ tabId, payload }`（`rpc:events`、`session-info:update`、`commands:update`… 12 条）
   - `{ request, tabId }`（`extension-ui:request`、`host-tool:call`、`host-uri:request`）

   发错不会报错，只会静默收到 `undefined`。`src-tauri/src/lib.rs` 的 `event_payload()` + 4 个单测
   把这条锁住了；改事件时先改那里的测试。
4. **`commands:update` 的 payload 是内层数组**，不是整个帧。
5. **能力探测决定 argv**：`get_settings_schema` / `terminal_list` / `browser_host_start` / `get_jobs`
   探测结果 + `--help` 文本决定是否传 `--chat`/`--no-auto-resume`/`--session`/`--no-open`。
   stock 二进制走降级分支（`--session-dir` 隔离），绝不传它不认识的 flag。
6. **denylist 是 pair-aware 的**：支持 `--flag=value`，只对 7 个 valued flag 吞下一个 token，
   非被拒 valued flag 的值原样保留（`--append-system-prompt --session` 里的 `--session` 是值不是 flag）。
7. **超时表照抄上游** `RPC_COMMAND_TIMEOUTS`：bash/eval/compact/handoff/plan_approval/switch_leaf/login
   660 s，export_html/reload_plugins 120 s，reset credits 60 s，set_model/cycle_model/get_transcript/
   get_messages 30 s，其余 8 s。渲染层会自己算 `timeoutMs` 传下来，`ipc.rs` 必须透传。
8. **agent 是常驻子进程，会话是线程**（对标 `openai/codex` 的 `app-server`：`thread/start`/`thread/resume`）。
   一个 tab 持有一个 sidecar；同 workspace 切换会话用 `switch_session` 原地换线程，**不再为每次点击新起进程**。
   跨 workspace 不能原地切（sidecar 返回 `cancelled:true`），才新开 tab。
9. **归属只认 sidecar 明确确认的切换**：`success:true && cancelled:false` 才更新 `tab_sessions` 与 pool
   ownership；`get_state` 返回的 `sessionFile` 用来记录 fresh tab 自己的会话，避免被回收后恢复成新会话。
10. **事件形状有三种**（不是两种）：`{tabId,payload}`、`{request,tabId}`、终端的 `{tabId,frame}`。
    `src-tauri/src/lib.rs` 的 `event_payload()` 与单测锁住这三种。
11. **定时任务的运行 argv 由代码控制**，不受用户 launch profile 影响：审批模式跟随任务的工具策略
    （`read_only` → `--approval-mode always-ask --tools read,glob,grep,lsp,web_search,todo`、`workspace` → `write`、
    `full` → `yolo`），并且 `--approval-mode/--yolo/--auto-approve/--plan-yolo/--tools/--no-tools/--model/--continue`
    会从用户 flag 里剥掉。每个 run 有自己的 `--session-dir`，不可能续上交互会话。单测里塞了 yolo profile 验证。
12. **run 必须有终态**。`run-now` 起进程后由监督循环读 stdout 到 `agent_end`/EOF/超时，再
    `finish_run(status, report, endedAt)`；取消时先置标记再 kill，所以记成 `cancelled` 而不是 `failed`。
    报告只从 `agent_end.messages` 取（`message_update` 是 delta，只带 role，取不到正文），`turn_end` 作兜底。
13. **host tool 的定义必须是 `parameters`**，不是 `inputSchema`：sidecar 会拒 `Host tool "x" must provide a JSON
    Schema object`。定义从上游 TS 用 `scripts/extract-host-tools.mjs` 生成到 `host_tools_defs.json`，Rust 用
    `include_str!` 加载，避免手抄漂移。
14. **退出必须同步清理**。`app:quit` 跑在 async runtime 上，在里面 `block_on` 会 panic
    （"cannot block the current thread from within a runtime"）；改用 `StatsServer::terminate_now()`（同步 kill）。
    关窗与退出都先 `bench.abort()` → 关所有 tab → 停 stats 再退；强杀仍会留孤儿（需 Job Object）。
15. **tab 顺序不能靠 HashMap**。`pool.tabs()` 按 id 数字后缀排序（`tab-2` 在 `tab-10` 前），否则
    `tab:get-all` 吐给渲染层的顺序随机，违背「acquisition order」契约。
16. **布局持久化要在切 active 之后**：`tab:spawn` 先 `start_tab` 再设 active，persist 必须放最后，
    否则存下的 `activeIndex` 指向旧 tab。

---

## 5. 如何运行

```bash
# 前端依赖（已用 npmmirror，无需代理）
bun install

# 类型检查 / 前端构建
bunx tsc --noEmit
bunx vite build

# Rust 测试（需要 MSVC 环境，见下）
scripts\with-msvc.cmd cargo test --workspace

# 协议探针（需要 sidecar 二进制）
scripts\with-msvc.cmd cargo run -p omp-agent --example probe -- resources/omp.exe

# 开发态起窗（推荐：注入 MSVC/PATH/OMP_SIDECAR，并开 WebView2 CDP 9222）
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/dev.ps1
#   等价手动：$env:OMP_SIDECAR="D:\rust-omp\resources\omp.exe" 后 bunx tauri dev
#   （dev.ps1 优先取仓库内 resources/omp.exe，找不到才回落到别的 checkout）

# 驱动 UI（CDP，无需截图）
node scripts/cdp-eval.mjs "await window.omp.tabs.list()"
node scripts/measure-click.mjs "先看下当前项目"

# 性能采样（进程树 RSS 按 app/webview2/sidecar/stats 拆分）
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/measure-perf.ps1
# 已起窗后只采样当前状态：
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/measure-perf.ps1 -SampleOnly -Label 4tabs

# 从上游 TypeScript 重新生成 host tool schema（改完 .research 后跑）
node scripts/extract-host-tools.mjs
```

### sidecar 二进制重建（`resources/omp.exe`，gitignored）

```bash
# 1) 下载 fork 版 portable（194,927,027 B）
curl -L --proxy http://127.0.0.1:7897 -o .resources/omp-portable.exe \
  https://ghfast.top/https://github.com/nornzach/oh-my-pi-gui/releases/download/v0.9.35/omp-0.9.35-portable.exe
# 2) NSIS 解包（7-Zip 在 C:/7zip/7z.exe，winget 装不上时用它）
C:/7zip/7z.exe x -y -o.resources/portable .resources/omp-portable.exe
C:/7zip/7z.exe x -y -o.resources/app ".resources/portable/\$PLUGINSDIR/app-64.7z"
# 3) 拷到项目
cp .resources/app/resources/omp.exe resources/omp.exe
```

---

## 6. 未完成与风险

| 项 | 说明 | 风险 |
|---|---|---|
| **浏览器面板** | `browser:state`/`browser:key` 是 GUI 侧状态；sidecar 只有 `browser_host_start/stop` 的 relay websocket。要实现内置 webview host（离屏 + CDP relay），是 Phase 8 最大的一块，不是接线活 | **高**（唯一整块缺失的用户面） |
| phantom tab | 启动后偶发一个无人点击的 tab。来源日志已就位（`ensure_tab_started` 打 origin/cwd/kind/session/tabs，`tab:spawn` 打完整 launch 形状），但本轮 8 次干净启动只出现 `tab-1`，**未复现** | 中（待复发） |
| 空闲 sidecar 租约矩阵 | 只按上限 LRU 回收；待审批/终端/浏览器/MCP/队列这些"不能回收"的租约未细分 | 中 |
| 会话归档/树 | 重命名/删除/搜索/原地切换已验；归档、批量、运行中跳过、branch/fork、导出未接 | 中 |
| provider 登录 | `login` RPC 走通用转发，但 OAuth 需要浏览器回调，未做端到端 | 中（需人工点） |
| 定时任务通知/insights | `scheduler:get-notices` 返回空、insights 是空壳；run 终态与报告已闭环 | 低 |
| 打包 | `bundle.resources` 映射 `omp.exe`；NSIS 未构建，`Romp.exe` 命名/图标/资源路径都未在真实安装包上验 | 中 |
| 深链/托盘 | `omp://` 与托盘未接 | 低 |
| 终端面板 UI | 帧路由已实测（`terminal_opened/output` 以 `{tabId,frame}` 到达），xterm 视觉未人工确认 | 低 |
| vendor 契约门 | `argv_matches_capabilities` 已通过（5 条）；`stats_paths_match_vendor` / `rpc-types` 对账需要 fork 的 `packages/stats/src/server.ts` 与 `coding-agent/src/modes/rpc/rpc-types.ts`，本机没有 | 中 |
| 调试器 | codelldb 已解到 `C:/Users/666/.omp/tools/codelldb/`，`.omp/dap.json` 已写，断点未跑 | 低 |
| 强杀孤儿 | 正常退出已闭环（实测 0 孤儿）；`Stop-Process` 强杀主进程仍留 sidecar（需 Job Object） | 低 |
| `markdown-links.test.tsx` | 已在 `vitest.config.ts` 排除（它 import main 进程的 `toAbsolutePath`） | 低 |
| MSVC 路径 | 用 `scripts/dev.ps1` / `scripts/with-msvc.cmd`；直接 `cargo` 会命中 Git 的 `link.exe` | 中 |
| dev.ps1 端口占用 | 反复起停会留下 vite 占住 1420（`beforeDevCommand` 报错）。先杀端口再起 | 低 |

---

## 7. 环境与网络（本机实测）

- GitHub raw：`https://ghfast.top/https://raw.githubusercontent.com/...`（0.8 s）优先，jsDelivr 次之。
- `git clone` 直连会被 reset；走 `-c http.proxy=http://127.0.0.1:7897` 最稳（镜像前缀也能 clone 但慢）。
- cargo 走 `sparse+https://rsproxy.cn/index/`（`~/.cargo/config.toml` 已配）；npm/bun 走 npmmirror。
- 7-Zip 在 `C:/7zip/7z.exe`（winget 装 7zip 会卡住，直接用官方 exe 注释式安装）。
- MSVC：Build Tools 17.14.37827.9（MSVC 14.44.35207）+ Win11 SDK 10.0.26100.0 已装；用 `scripts/dev.ps1` 注入 PATH/LIB/INCLUDE。
- 技能库：`grilling` / `ponytail` / `trellis` / `replica-*`（11 个）/ `doc-drift` 已装，SOP 见 `eng-dev-sop`。

---

## 8. 建议的下一步顺序

按价值排（`docs/parity.md` 是逐条账本，完成一条就改该行状态并附证据）：

1. **浏览器 webview host**（Phase 8，最大一块）。契约：`browser:state` / `browser:key` 由 GUI 发出，
   sidecar 只提供 `browser_host_start/stop` 的 relay websocket。要 Tauri 子 webview + 离屏休眠 +
   CDP relay，且 agent 的 `browser_*` 工具要能驱动它、截图要能回传。这是唯一整块缺失的用户面。
2. **NSIS 打包**（Phase 9）。一次 `cargo tauri build` 会同时暴露 `bundle.resources` 的 `omp.exe` 路径、
   图标、`externalBin` 与产品名（`Romp.exe`）的问题，比逐个猜便宜。
3. **定时任务通知 / insights**（Phase 7 收尾）：`scheduler:get-notices` 与 insights 目前是空壳。
4. **phantom tab**（Phase 3）：日志已就位，等复发时看 `runtime.log` 的 `ensure_tab_started` /
   `spawning sidecar` 两条来源记录即可定案，不需要再加 instrument。
5. **sidecar 租约矩阵**（Phase 3）：待审批 / 终端 / 浏览器 / MCP / 队列这些"不能回收"的状态要挡住 LRU。
6. **会话归档 / 树 / branch-fork / 导出**（Phase 3+4）。
7. **provider 登录/登出**（Phase 5）：OAuth 需要浏览器回调，要先人工走一遍。
8. **契约门**（Gates）：`vendor/` 放 fork 的 `rpc-types.ts` 与 `packages/stats/src/server.ts`，
   跑 `stats_paths_match_vendor`。`argv_matches_capabilities` 已通过。

### 本轮已闭环、无需再查的项

- 同 workspace 点会话不新增 tab、sessionId 真的切换（`switched=true`，tabs 1→1）
- fresh tab 会话隔离（tab-2 拿到全新 session 文件，`--no-auto-resume` 没被忽略）
- 模型指示器（composer 显示 `group/never-stuck · 超高 · 快速`）
- 设置页 schema 渲染 + `set_setting` 写路径（原值写回 `success:true`）
- 命令面板（`get_available_commands` 87 KB 命令表已推送到渲染层）
- 统计服务自愈（杀子进程后 329 ms 换端口）、同窗口数字与 `stats.db` 三项完全一致
- 会话搜索 snippet 的 U+0001/U+0002 标记确实穿过 IPC 到达渲染层
- 退出无 panic、0 孤儿

---

## 9. Git 与许可

- 远端：`git@github.com:matata2017/omp-desktop.git`（private），分支 `master`。
- 进度快照：`2026-11-04` tag（未推送）。`2026-11-05` 之后的推进全部在 `master` 上，
  单日新增 12 个提交（stats / bench / scheduler / 会话搜索 / 布局恢复 / models / editor / system / argv 契约门）。
- `.gitignore` 覆盖 `target/`、`node_modules/`、`.research/`、`.resources/`、`resources/*.exe`、
  `.probe/`、`replica/screens/`、`src-tauri/gen/schemas/`，以及侦察期的 JSON/清单/zip 临时产物。
- 本机 SSH 认证为 `matata2017`（`~/.ssh/config` 走 `ssh.github.com:443`）。

许可（本仓库**同时**承载三方代码，必须随附声明）：

| 目录 | 来源 | 许可 |
|---|---|---|
| `src/renderer`、`src/shared` | `nornzach/oh-my-pi-gui` v0.9.35 | MIT（`LICENSES/oh-my-pi-gui-MIT.txt`，含原版权行） |
| `.trellis/`、`.omp/skills/trellis-*` | `mindfold-ai/Trellis` 模板 | AGPL-3.0（`LICENSES/trellis-AGPL-3.0.txt`） |
| `crates/`、`src-tauri/`、`src/bridge/`、`replica/`、`docs/` | 本工程 | MIT（`LICENSE`） |

发布或再分发时，三条声明都要一起带上；`src/renderer` 的字节级修改仅限 `main.tsx`、`index.html`、`ChatStream.tsx`（加载门）、`use-session-switch.ts`（原地切换），其余保持上游。
