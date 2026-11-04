# 交接文档（handoff）

最后更新：2026-11-04。目标：把 `nornzach/oh-my-pi-gui`（Electron 35 + React 19，v0.9.35，MIT）
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
| 1 | Rust workspace + RPC 传输 + sidecar | **完成**（本机 65 项 Rust 测试 + 6 条协议探针全通过） |
| 2 | Tauri 外壳 + `window.omp` shim | **完成**：Romp 起窗、状态 ready、会话列表、Inspector、完整 prompt→流式→工具卡→agent_end；终端帧已路由并实测 |
| 3 | 会话 / 多标签 | **进行中**：原地 `switch_session`（Codex 式）、sessions:list/delete/rename/search、family 折叠、per-tab client/batcher、LRU 回收 |
| 4–9 | 对话面、模型设置、统计、定时任务、终端面板 UI、浏览器、打包 | 未开始（浏览器是 GUI 内置 webview，不是接线活） |
| R | replica 侦察 | **完成** |

### 已验证的事实（不是推断）

```
bunx tsc --noEmit                                → 0 error
bunx vite build                                  → 成功，vendor 分块与上游一致
bun run test                                     → 186 files / 1571 passed
cargo test --workspace                           → 65 passed（11 suites）
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

## 2. 目录结构

```
crates/
  omp-core/     agent/data 路径、TabId/WindowId、SessionState 解析、Result 别名
  omp-rpc/      frame.rs（分块重组）client.rs（id 关联 + 超时表）transport.rs（NDJSON 泵）
  omp-agent/    argv.rs（能力门控 argv）sidecar.rs（进程/写通道/stderr）batcher.rs（32 ms 合批）
                pool.rs（tab→进程、deferStart、F-OWN 归属、F-UI-ORIGIN 回包路由）
  omp-rpc/examples/probe.rs    协议探针（Phase 1 验收）
  omp-session/  会话索引：32 KiB 头/尾窗口解析、标题/首条消息/计数/状态、(size,mtime) 缓存
scripts/         dev.ps1（注入 MSVC/PATH/OMP_SIDECAR + CDP 端口）、with-msvc.cmd、install-msvc.ps1
                 cdp-eval.mjs / cdp-cmd.mjs / measure-click.mjs（WebView2 9222 驱动）、capture-window.ps1
vitest.config.ts + scripts/test-locale.ts（上游缺，导致 268 个 locale 测试失败，已补）
src-tauri/      lib.rs（状态/事件路由）ipc.rs（唯一命令 ipc_invoke）capabilities/ tauri.conf.json
src/renderer/   上游 v0.9.35 原样拷贝（567 文件 / 111,400 行），仅改 main.tsx、index.html、ChatStream.tsx（加载门）、use-session-switch.ts（原地切换）
src/shared/     上游类型与 rpc-client.ts 原样拷贝（合同的唯一来源）
src/bridge/omp-api.ts   Electron preload 的 Tauri 移植：OmpApi → ipc_invoke
replica/        侦察产物（recon.md、parts/*、features.csv）
docs/parity.md  复刻对照账本
.trellis/       Trellis 任务台账（01-tauri-shell 为本阶段任务）
resources/omp.exe  fork sidecar（gitignored，需按第 4 节重建）
```

---

## 3. 关键设计决策

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

---

## 4. 如何运行

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
#   等价手动：set OMP_SIDECAR=D:/ompgui/resources/omp.exe 后再 bunx tauri dev

# 驱动 UI（CDP，无需截图）
node scripts/cdp-eval.mjs "await window.omp.tabs.list()"
node scripts/measure-click.mjs "先看下当前项目"
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

## 5. 未完成与风险

| 项 | 说明 | 风险 |
|---|---|---|
| 多 tab 剩余 | spawn/close、per-tab client/batcher、上限 6 + LRU 回收已落地；布局持久化/多窗口未做 | 中 |
| 空闲 sidecar 释放 | 现在只按上限 LRU 回收，租约矩阵（待审批/终端/浏览器/MCP/队列）未细分 | 中 |
| 模型指示器 | composer 仍显示“选择模型”而 sidecar 实际有模型（`get_state.model` 有值）；renderer model store 未被灌入 | 中 |
| 会话操作 | 重命名/删除/搜索已实测；归档、批量、运行中跳过、会话树/branch/fork 未接 | 中 |
| phantom tab | 启动后偶发出现一个无人点击的 tab（07:51:40 的 pinrong tab-2）；来源未定位，会污染 UI 测量 | **高**（未解释） |
| fresh tab 会话 | argv 已带 `--no-auto-resume`，但一次 `get_state` 仍报旧会话；根因未确认 | 中 |
| 未实现通道 | 仍缺：scheduler:*、models:*、bench:*、system 对话框/剪贴板/通知、editor:open-external、conversations:search、updater:* | 低（可见但会缺功能） |
| 终端面板 UI | 帧已路由且实测收到 `terminal_opened/output`；xterm 面板视觉未人工确认 | 低 |
| 浏览器面板 | `browser:state`/`browser:key` 是 GUI 侧发出的，sidecar 只有 `browser_host_start/stop`；要实现内置 webview host，不是接线活 | 中 |
| `markdown-links.test.tsx` | 已在 `vitest.config.ts` 排除：它 import main 进程的 `toAbsolutePath` | 低 |
| 打包 | `bundle.resources` 映射到 `omp.exe`；`Romp.exe` 不再撞名；NSIS 未构建 | 低 |
| 调试器 | codelldb 解到旧用户目录，`.omp/dap.json` 已写，未跑过断点 | 低 |
| MSVC 路径 | 用 `scripts/dev.ps1` / `scripts/with-msvc.cmd`；直接 `cargo` 会命中 Git 的 `link.exe` | 中 |
| vitest 配置 | 原仓库缺 `vitest.config.ts` / `scripts/test-locale.ts` | 已修复；1571 测试通过 |

---

## 6. 环境与网络（本机实测）

- GitHub raw：`https://ghfast.top/https://raw.githubusercontent.com/...`（0.8 s）优先，jsDelivr 次之。
- `git clone` 直连会被 reset；走 `-c http.proxy=http://127.0.0.1:7897` 最稳（镜像前缀也能 clone 但慢）。
- cargo 走 `sparse+https://rsproxy.cn/index/`（`~/.cargo/config.toml` 已配）；npm/bun 走 npmmirror。
- 7-Zip 在 `C:/7zip/7z.exe`（winget 装 7zip 会卡住，直接用官方 exe 注释式安装）。
- MSVC：Build Tools 17.14.37827.9（MSVC 14.44.35207）+ Win11 SDK 10.0.26100.0 已装；用 `scripts/dev.ps1` 注入 PATH/LIB/INCLUDE。
- 技能库：`grilling` / `ponytail` / `trellis` / `replica-*`（11 个）/ `doc-drift` 已装，SOP 见 `eng-dev-sop`。

---

## 7. 建议的下一步顺序

1. **先定位 phantom tab**：给 `tab:spawn` 加来源日志（cwd/sessionPath/调用方），确认启动后那个无人
   tab 是谁开的；未定位前所有 UI 测量都不可信。
2. **人工确认两处手感**：① 干净窗口里点同 workspace 会话，确认 tab 数不增、sessionId 切换、首屏内容出现；
   ② 打开终端面板，确认 xterm 真的出字。
3. **fresh tab 会话根因**：确认是 renderer 启动后切换，还是该 build 忽略 `--no-auto-resume`；必要时改用
   上游 `tab-spawn.ts` 的 fresh 会话创建路径。
4. **浏览器面板**：`browser:state`/`browser:key` 是 GUI 侧状态，需要实现内置 webview host
   （sidecar 只提供 `browser_host_start/stop` 的 relay websocket）。
5. **剩余通道**：`scheduler:*`、`models:*`、`bench:*`、system 对话框/剪贴板/通知、`updater:*`、
   `conversations:search`（SQLite 索引）。
6. **契约门**：`vendor/` 放 fork 的 `rpc-types.ts` 与 `stats/server.ts`，跑 `stats_paths_match_vendor`
   与 `argv_matches_capabilities` 两个测试。
7. Phase 4–9 按 `docs/parity.md` 逐 area 推进，每完成一条把该行状态从 `todo` 改成 `done` 并附证据。

---

## 8. Git 与许可

- 远端：`git@github.com:matata2017/omp-desktop.git`（private），分支 `master`。
- 今日快照 tag：`2026-11-04` —— Romp 0.1.0-dev（Tauri 外壳 + RPC 传输 + 会话索引 + 原地会话切换 + 终端帧路由 + 会话增删改查）。
  该 tag 只记录当天进度，**未推送**；后续功能继续在 `master` 上推进。
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
