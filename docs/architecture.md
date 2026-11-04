# 架构设计：Rust 常驻服务 + 可回收 agent

> 参照 `l0ng-ai/openidea` 的技术栈选择，结合本仓库现状制定。
> 目标：空闲内存接近纯 Rust 桌面应用；tab 数不放大内存；agent 只在活动会话需要时存在，空闲释放。
> 产品名：**Romp**；内部 `omp-*` crate / `OMP_SIDECAR` 指上游 omp sidecar 与协议，不是产品名。
> 交付平台：当前 **Windows x64**；macOS/Linux 为 post-1.0。平台差异集中在 `src-tauri` 适配层，业务 crate 不写死 Windows，但不为后置平台做构建/签名验收。

## 0. 五条决定

1. **Rust 拥有状态与 I/O，WebView 只是视图。** 会话、文件、搜索、Git、终端、浏览器、定时任务都在 Rust 服务里；renderer 不直接持有这些资源。
2. **agent 是子进程，不是应用运行时。** 一个活动会话一个 `omp --mode rpc`；回合结束后的空闲宽限期释放；tab 本身不拥有进程。
3. **tab 是会话的视图。** tab 只在成为活动路由时申请一个 sidecar 槽位；聚焦期间保持 warm，失焦且租约归零后释放。打开/切换时 transcript 先由 Rust 缓存渲染，composer 在 sidecar ready 前保持现有禁用态。
4. **不让 tab 数进入内存公式。** 进程数 = 活动回合数 + 至多 1 个空闲预热槽；默认并发回合上限 2，超出排队并显示"等待 agent"。
5. **保留上游 renderer 复用。** 不重写 React、不换纯 Rust UI；所有改动在 Rust 侧与 `src/bridge/omp-api.ts`。唯一例外是已记录的 `ChatStream.tsx` 加载门（ready 但 transcript 未到时继续显示骨架，避免空会话页闪烁）。

## 1. 从 OpenIDEA 学什么

| 学 | 不学 |
|---|---|
| Tauri 2 + WebView 前端 + Rust 后端的分层 | 34 个 crate 的全套 IDE 抽象 |
| Rust 域 crate（vfs/search/git/terminal/lsp/dap） | ropey/tree-sitter（我们没有代码编辑器） |
| agent 子进程按任务启停，空闲释放 | ACP 协议替换现有 renderer 数据层 |
| `tauri-specta` 生成类型 | WASM 插件（当前不解决内存问题） |
| `perf/budgets.json` 式预算门禁 | 把 169MB 当成必须达到的指标 |

复用具体库而不是复用 crate：`gix`、`grep-searcher`、`nucleo-matcher`、`redb`、`portable-pty`、`notify`、`ignore`、`specta`。

## 2. 目标架构

```
┌────────────────────────────────────────────────────────────┐
│ Tauri shell (src-tauri)                                    │
│  窗口/菜单/托盘/深链/更新；commands/<domain>.rs 只做适配     │
├────────────────────────────────────────────────────────────┤
│ omp-services (in-process Rust, 常驻)                        │
│  omp-session   JSONL 索引 + redb 缓存 + transcript/search   │
│  omp-vfs       ignore + notify + 文件读写                    │
│  omp-search    grep-searcher + nucleo                       │
│  omp-git       gix                                          │
│  omp-terminal  portable-pty                                 │
│  omp-browser   WebView2 子 webview + CDP relay              │
│  omp-mcp       MCP 注册/代理                                 │
│  omp-scheduler headless 运行                                 │
├────────────────────────────────────────────────────────────┤
│ omp-agent (进程管理器)                                       │
│  按活动会话 spawn `omp --mode rpc`；失焦/空闲释放             │
├────────────────────────────────────────────────────────────┤
│ WebView2: src/renderer（上游 v0.9.35，原样）                 │
└────────────────────────────────────────────────────────────┘
```

`src-tauri` 不写业务逻辑；每个 `commands/<domain>.rs` 只做 `IPC -> service` 的薄适配，和 OpenIDEA 的 `ide-host` 一致。

## 3. 进程生命周期（状态机）

| 状态 | 触发 | 进程 | renderer |
|---|---|---|---|
| `detached` | tab 存在但未激活；或已释放 | 无 | 读 `omp-session` 缓存 |
| `starting` | tab 激活（聚焦） | 1 个 | "启动中/恢复中" |
| `running` | 回合进行中 | 1 个 | 流式帧 |
| `idle` | `agent_end` | 1 个（warm 计时） | 静态 transcript |
| `releasing` | （失焦宽限到期 **或** 空闲超时）**且** 全部租约为零（含焦点租约） | 退出中 | 不变 |
| `queued` | 达到并发回合上限 | 0 | "等待 agent" |
| `error` | 启动/恢复失败 | 0 | 可重试 |

**为什么不是首次 prompt 才启动**：`InputArea.tsx:1174-1177` 的发送按钮要求 `status === "ready"`，`:277-278` 的 slash 命令加载同样要求；`stores/session.ts:111` 初始状态是 `starting`，只有 sidecar 事件（`use-rpc-events.ts:642-664`）会把它推到 `ready`。没有 sidecar 的 detached tab 会永远禁用发送，形成唤醒死锁。M1 因此选择**激活时启动**：不伪造 ready、不改 renderer。代价是切回已释放的 tab 要等一次启动/`--resume`，transcript 仍立即可见。若以后要"零进程直到首次 prompt"，需由 Rust 提供完整的 detached `get_state` / `get_available_commands` 虚拟状态，列为可选优化。

**释放租约（任一非零则不释放）：**

- **焦点租约**：当前活动 tab 在聚焦期间始终持有。聚焦 tab 不因空闲超时被回收——否则用户一直停在该 tab 时不会产生“再次激活”事件，`InputArea.tsx:1177` 会永远禁用发送。首版只回收失焦会话；窗口失焦后的主动释放/回焦预热列为后续优化。

- 活动回合 / 未完成的 `prompt`；
- 待审批 / 待回答的 extension UI；
- steering / follow-up 队列非空；
- `get_jobs` 有 running/parked；
- 有 live `terminal_*` PTY；
- 有 Browser 页面或 CDP attach；
- MCP/LSP 调用在飞；
- Collab / Live voice 活跃；
- 会话尚未落盘。

**恢复：** 释放只发生在失焦后；再次激活该 tab 时用 `--resume <session>` 重启，transcript 由缓存立即显示，composer 等 `ready` 后启用。聚焦 tab 不会进入 detached 状态。
**默认关闭：** 先只记录"本可释放但被哪个租约挡住"；恢复矩阵（transcript、todos、plan、queue、model/thinking、分页游标、终端、浏览器、MCP、后台 job）全部通过后再开。

## 4. 为什么这样会快

- **tab 不各自持有进程**：打开/切换先读缓存；活动路由启动一个 sidecar，聚焦期间 warm，失焦且租约清零后释放。
- **搜索不依赖 agent**：`omp-search` 在进程内跑 grep/nucleo；`omp-session` 维护跨会话索引。
- **终端不依赖 agent**：`portable-pty` 在 Rust 侧，sidecar 释放不影响 shell。
- **文件树不依赖 agent**：`omp-vfs` 用 `ignore` 走目录，`notify` 增量更新。
- **事件少拷贝**：sidecar 帧 → Rust 解析/批处理（沿用 32ms batcher）→ 一次 IPC 批量推给 renderer。
- **启动**：窗口 `create:false`，pre-paint 后 `show()`；release LTO + `codegen-units=1` + `opt-level=3`，dev 依赖 `opt-level=2`。

## 5. IPC 与类型

- 保留 `ipc_invoke(channel, payload)`：renderer 和 `src/bridge/omp-api.ts` 不改，`src/shared/ipc-types.ts` 仍是通道名的唯一权威。
- 新增的 Rust 命令/事件用 `tauri-specta` 生成 `src/bindings.ts`；`src-tauri` 内部只用生成类型。
- 事件形状继续由 `src-tauri/src/lib.rs` 的 `event_payload()` 测试锁住；任何新事件先加测试。

## 6. 预算（先测，后定）

不套用 OpenIDEA 的 169MB：那是 Windows 11 / 32 核 / 62GB / freqtrade 770 文件 / 暖缓存的单次基准。也不把 244MB 的 `omp.exe` 文件大小当 RSS。

M0 在本机测：

- 冷启到窗口 / ready；
- USS（应用 + WebView2 子进程 + sidecar）分状态：无 tab、1 tab 空闲、1 回合、2 tab；
- 空闲 CPU；退出耗时；
- 每次释放/恢复的延迟。

预算在测量后写入 `perf/budgets.json`，并由 CI 门禁；此前不写死 MB 数字。

## 7. 迁移顺序

| 阶段 | 内容 | 验收 |
|---|---|---|
| M0 | 性能/内存基线脚本 | 本机数字可重复 |
| M1 | `omp-session` + tab-as-view；只有活动路由持有 sidecar | 1 个焦点 tab ≤1 sidecar 且聚焦期间保持 warm；失焦且租约全零才释放；N 个未激活 tab = 0 sidecar；切回已释放 tab 先出缓存 transcript，composer 显示恢复中 |
| M2 | `omp-agent` 租约 + 观测（不 kill） | 日志能解释每次未释放原因 |
| M3 | 终端/浏览器/MCP 迁到 Rust | 释放后这些资源仍可用 |
| M4 | 搜索/索引/Git 迁到 Rust | Files/Search/Git 面板无 sidecar 可用 |
| M5 | `specta` 类型 + CI 预算 | 类型漂移为 0；预算回归阻断 |

M1 是分水岭：**先停止"每 tab 一个 sidecar"的设计，再谈释放**；M1 采用激活时启动、聚焦保持 warm，因此 renderer 的 `ready` 门禁不需要改。M2/M3 的顺序不能反：资源没迁走就释放，等于用 `--resume` 赌 PTY/浏览器/MCP 能恢复。

## 8. 非目标

- 重写 omp agent 为 Rust（数月到数年；不在本次范围）。
- 换掉 React renderer / 上 egui、iced（破坏上游复用与 100 条功能账本）。
- ACP 替换现有 RPC 数据层（renderer 依赖现有协议；ACP 只作为未来接其他 agent 的缝）。
- WASM 插件（不解决当前内存问题，后置）。
