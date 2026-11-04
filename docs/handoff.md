# 交接文档（handoff）

最后更新：2026-11-04。目标：把 `nornzach/oh-my-pi-gui`（Electron 35 + React 19，v0.9.35，MIT）
的功能复刻成 Rust/Tauri 2 桌面应用 `omp desktop`，常驻内存与冷启优于 Electron 版。

权威输入有三份，冲突时按此顺序：

1. `replica/recon.md` + `replica/parts/*`（**建什么**：22 屏 / 54 对话框 / 26 流程 / 28 实体 / 100 条功能，逐条带 `文件:行` 证据）
2. `docs/parity.md`（**做到哪**：逐条状态账本，机器可读源 `replica/features.csv`）
3. 已批准的实施计划（**怎么建**：Phase 0–9 的步骤与验收）

---

## 1. 当前状态

| Phase | 内容 | 状态 |
|---|---|---|
| 0 | 工具链与镜像 | **完成** |
| 0.5 | LSP + 调试器 | 配置完成，调试会话未验证 |
| 1 | Rust workspace + RPC 传输 + sidecar | **完成**（协议门实测通过） |
| 2 | Tauri 外壳 + `window.omp` shim | 代码完成，**窗口烟测未做** |
| 3–9 | 会话/多标签、对话面、模型设置、统计、定时任务、终端与浏览器、打包 | 未开始 |
| R | replica 侦察 | **完成** |

### 已验证的事实（不是推断）

```
cargo test -p omp-core -p omp-rpc -p omp-agent   → 3 + 23 + 26 = 52 passed
cargo test -p omp-desktop                        → 7 passed（事件信封形状）
bunx tsc --noEmit                                → 0 error
bunx vite build                                  → 成功，vendor 分块与上游一致
cargo run -p omp-agent --example probe -- resources/omp.exe
  ready: protocolVersion=1 v2=true
  get_state / get_available_models / get_settings_schema
  terminal_list / get_jobs / get_context_report → 全 success=true
```

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
src-tauri/      lib.rs（状态/事件路由）ipc.rs（唯一命令 ipc_invoke）capabilities/ tauri.conf.json
src/renderer/   上游 v0.9.35 原样拷贝（567 文件 / 111,400 行），仅改 main.tsx 与 index.html
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

---

## 4. 如何运行

```bash
# 前端依赖（已用 npmmirror，无需代理）
bun install

# 类型检查 / 前端构建
bunx tsc --noEmit
bunx vite build

# Rust 测试
cargo test -p omp-core -p omp-rpc -p omp-agent -p omp-desktop

# 协议探针（需要 sidecar 二进制）
cargo run -p omp-agent --example probe -- resources/omp.exe

# 开发态起窗（Vite 1420 + Tauri）
bunx tauri dev
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
| Phase 2 窗口烟测 | `bunx tauri dev` 尚未跑过；窗口是否起、renderer 是否拿到事件未验证 | **高**：事件信封/权限任一错都会「窗口在但没数据」 |
| `src-tauri/capabilities/default.json` | 已建（`core:default` 覆盖 event/window/webview/path），但**未在真实运行中验证** | 中 |
| Phase 3 多标签 | 现在只有一个 sidecar、`rpc:command-for-tab` 忽略 `tabId`；`SidecarPool` 已具备多 tab 能力但 `ipc.rs` 未接 | 中 |
| 事件批的 tab 归属 | `batch_tab` 是全局单值，多 tab 时要改成 per-tab batcher | 中 |
| 未实现通道 | 除 rpc/tab/prefs/fs/system/updater 外一律 `Err("unknown channel")`，渲染层按 try/catch 降级 | 低（可见但会缺功能） |
| 浏览器/终端 | Phase 8 前不可用 | 低 |
| `markdown-links.test.tsx` | 从 tsc/vitest 排除：它 import main 进程的 `toAbsolutePath`，main 已由 Rust 接管 | 低 |
| 打包 | `bundle.resources` 已映射 `omp.exe`，NSIS 未构建 | 低 |
| 调试器 | codelldb 解到 `C:/Users/666/.omp/tools/codelldb/`，`.omp/dap.json` 已写，未跑过断点 | 低 |

---

## 6. 环境与网络（本机实测）

- GitHub raw：`https://ghfast.top/https://raw.githubusercontent.com/...`（0.8 s）优先，jsDelivr 次之。
- `git clone` 直连会被 reset；走 `-c http.proxy=http://127.0.0.1:7897` 最稳（镜像前缀也能 clone 但慢）。
- cargo 走 `sparse+https://rsproxy.cn/index/`（`~/.cargo/config.toml` 已配）；npm/bun 走 npmmirror。
- 7-Zip 在 `C:/7zip/7z.exe`（winget 装 7zip 会卡住，直接用官方 exe 注释式安装）。
- MSVC：`Microsoft.VisualStudio.2022.BuildTools` 17.14 + Win10 SDK 26100，`cargo run` 已验证。
- 技能库：`grilling` / `ponytail` / `trellis` / `replica-*`（11 个）/ `doc-drift` 已装，SOP 见 `eng-dev-sop`。

---

## 7. 建议的下一步顺序

1. `bunx tauri dev` 起窗，肉眼确认：标题栏、会话列表、输入框；发一条 "list files in crates" 看流式
   文本与工具卡。**这一步不做完，后面所有 UI 工作都没有立足点。**
2. 若窗口无数据：先查 capabilities（事件权限）→ 再查 `event_payload` 形状 → 最后查 sidecar 是否 spawn。
3. Phase 3：把 `ipc.rs` 的 `rpc:command-for-tab`、`tab:spawn/close/set-view` 接到 `SidecarPool`，
   `batch_tab` 改 per-tab batcher。
4. Phase 4–9 按 `docs/parity.md` 逐 area 推进，每完成一条把该行状态从 `todo` 改成 `done` 并附证据。
5. 契约门：`vendor/` 放 fork 的 `rpc-types.ts` 与 `stats/server.ts`，跑 `stats_paths_match_vendor`
   与 `argv_matches_capabilities` 两个测试。

---

## 8. Git 与许可

- 远端：`git@github.com:matata2017/omp-desktop.git`（private），分支 `master`。
- `.gitignore` 覆盖 `target/`、`node_modules/`、`.research/`、`.resources/`、`resources/*.exe`、
  `.probe/`、`replica/screens/`、`src-tauri/gen/schemas/`，以及侦察期的 JSON/清单/zip 临时产物。
- 本机 SSH 认证为 `matata2017`（`~/.ssh/config` 走 `ssh.github.com:443`）。

许可（本仓库**同时**承载三方代码，必须随附声明）：

| 目录 | 来源 | 许可 |
|---|---|---|
| `src/renderer`、`src/shared` | `nornzach/oh-my-pi-gui` v0.9.35 | MIT（`LICENSES/oh-my-pi-gui-MIT.txt`，含原版权行） |
| `.trellis/`、`.omp/skills/trellis-*` | `mindfold-ai/Trellis` 模板 | AGPL-3.0（`LICENSES/trellis-AGPL-3.0.txt`） |
| `crates/`、`src-tauri/`、`src/bridge/`、`replica/`、`docs/` | 本工程 | MIT（`LICENSE`） |

发布或再分发时，三条声明都要一起带上；`src/renderer` 的字节级修改仅限 `main.tsx` 与 `index.html`。
