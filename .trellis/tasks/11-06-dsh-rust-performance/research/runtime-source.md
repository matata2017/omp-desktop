# DSH 运行时源码调查 — 进程/生命周期、流事件所有权、Cordis↔native 边界

**版本锚点（重要）**：本文全部结论来自本地快照 `D:/work/huanghao/dsh-src`，HEAD `687f282e8625c6b8d11ff58211a90524be2e4317`，package version `0.2.1-rc.2`。
上游 master 现为 `0.2.2-rc.1`（`typescript ^6.0.3`、`node >=24.9.0`），因此**任何数字与文件行号都必须在实现前对目标版本重新核对**，本文不代表当前官方最新版。
codegraph：`dsh-src` 与 `omp-desktop` 均无 `.codegraph/` 索引（`ls -d dsh-src/.codegraph` 失败），故按约定 fallback 为直接读源码。

标注约定：`[F]` = 源码可直接验证的事实；`[H]` = 假设/推断，必须实测；`[M]` = 由仓库内脚本/配置机械统计得出。

---

## 1. 真实进程拓扑（不是"一个 Electron App"）

`[F]` 桌面端分为 shell、独立 Host、renderer 和工具进程；agent 不在 UI renderer 中执行。以下仅表示通信关系，不是 OS 父子进程树。

```
Electron main (Node)
  └─ spawn(Electron 二进制作 Node 用, entry=@deepseek-ai/dsh-desktop-host/lib/index.js, stdio:[ignore,pipe,pipe,ipc])
       └─ Host 进程：runProfile('desktop') → 完整 Web 应用，监听 127.0.0.1:<随机端口>
            └─ Chromium renderer (dsh-app://app)  ←HTTP 静态/代理→ Electron main ←fetch→ Host
                                               ←WebSocket 直连 Host /api/remote.mux→
            └─ 工具子进程：bash / node-pty(ConPTY) / MCP / LSP / 原生 helper
```

证据：
- `apps/desktop/src/host-process.ts:186-201`：`spawn(this.node, ['--expose-internals', entry, runtimeDir, projectDir, primaryRuntime, ...], {stdio:['ignore','pipe','pipe','ipc']})`，`this.node` 是 **Electron 可执行文件以 Node 模式运行**（`desktopNodeEnvironment`）。
- `apps/desktop-host/src/index.ts:18-42`：`runProfile({profile:'desktop', args:['--no-open','--host','127.0.0.1','--port','0']})`；`:103-104` 把 `ctx.connection.authenticatedUrl(...)` 与 `ctx.webServer.collectIndexInjections()` 通过 `process.send({type:'ready', url, injections})` 交回 shell。
- `apps/desktop/src/main.ts:667-680`：`dsh-app://` 自定义协议处理器；`/`、`/index.html`、`/assets/*` 直接由磁盘 `dsh-web-frontend/dist` 提供，其余路径 `forwardWebRequest(...)`；`apps/desktop/src/web-document.ts:77-93` 用 Node `fetch` 转发到 loopback。
- `apps/desktop/src/main.ts:690-696`：`DESKTOP_IPC.boot` 返回 `{injections, streamBaseUrl: new URL(hostUrl).origin}`。
- `apps/web/src/main.ts:24-35`：renderer 在 `desktop.ready()` 返回前**不 boot 客户端插件树**（`await desktop.ready()` → 注入脚本 → `gate.resolve()`）。
- `packages/api/gateway/src/client/stream-client.ts:506-507` + `packages/client/connection/src/client/socket.ts:62`：WS 走 `__DSH_TRANSPORT__.streamBaseUrl`，即**绕过 Electron 代理直连 Host origin**。

`[F]` 生命周期由 shell 单方面拥有，且有明确的优雅/强制梯度：`host-process.ts:288-308` 先发 `{type:'shutdown'}` 等 10s，再 SIGTERM 等 5s，再 SIGKILL 等 5s；`backend-controller.ts:60-98` 把启动串行化并支持取消。退出前还会问 Host "有什么在跑"：`main.ts` 中 `DesktopHostProcess.inspectQuit()`，deadline 2s（`host-process.ts:49`）。

**对重写的含义**：桌面应用启动涉及 shell、Host 与页面/连接建立，不能据此断言每次新建会话都会启动这些进程。Rust 替换 Host 的收益必须分别计量启动、运行与内存；这里没有证明收益大小或上限。

---

## 2. Host 启动链路（逐步，带源码锚点）

`[F]` 顺序如下（`apps/cli/src/profile-boot.ts:271-346`）：

1. `installProxyFromEnvironment` → `composeProfile`：`loadProfile` + `createRuntimeResolution`（对整棵安装树做解析）+ 叠加 bundle 层/用户层/`--patch`（`:224-236`）。外层 shell 侧还有 `DesktopProjectManager.applyRelease()`（校验 release 描述符、迁移 profile 设置、创建 plugin profile、清理 link 投影）+ profile 文件锁（`apps/desktop/src/project-manager.ts:82-121`）。
2. `boot()`（`packages/boot/app-boot/src/index.ts:1200-1241`）：`new Context()` → `ctx.plugin(Loader)` → `prepare`（注入 `profileContext`、`PluginPackages` 解析、`provideCmdline`）→ `mountRootInclude`（挂整棵 YAML 树）→ `await ctx.get('loader')?.await()` → `auditStartupEntries`。
3. **每个 entry 都是一次独立动态 `import()`**：`vendor/loader/src/config/tree.ts:112-126`。
4. 树挂完 → Web 服务 listen（`port: 0`）→ `ready` IPC。
5. renderer 拿 injections → `boot.ts:49-103`：`__DSH_BOOT__` → `moduleLoader.create()` → `prefetchImmediateTier()`（只预取 `immediately` 行，`:116-122`）→ `bootClient`（**浏览器内第二棵 Cordis 树**）→ `mountClient`（React 挂载）。

`[M]` 该 profile 的插件行数（`packages/bundle/base` + `packages/bundle/web-app`，template 见 `packages/boot/app-boot/src/profile.ts:179-195`，desktop 使用 `WEB_PROFILE`，见 `apps/desktop/src/project-manager.ts:142,175`）：
- base：`- id:` insert 96 行，其中 `disabled:` 9 行 → 约 87 个活动 entry；
- web-app：insert 90 行，其中 `disabled:` 26 行 → 约 64 个活动 entry；另有 26 个顶层行是**对 base 行的 config patch**（不是新 entry）；
- 合计 **≈150 个活动 host plugin fiber / 每次启动**；YAML 内 `!!js` 求值表达式 base 16 处、web-app 13 处（挂载时执行，且部分表达式依赖 `ctx.get('profileContext')`，会形成"先有服务再解析该行"的顺序依赖）。
- 浏览器侧：`[M]` 79 个 package 在 `package.json` 声明 `dsh.client`（即 79 个懒加载 client entry）。

`[F]` **产物形态**：每个包由 tsdown 打成 `lib/index.js`（`tsdown.config.ts`，`format:['esm']`、`platform:'node'`、`target:'es2024'`、`dts:false`），无打包合并成单文件、无字节码。
`[F]` **全仓库没有任何启动期缓存/快照机制**：`grep -rn "enableCompileCache|NODE_COMPILE_CACHE|v8-compile-cache|Sea\."` 在 `apps/ packages/ scripts/` 命中 0 处（仅命中不相关的 "snapshot" 注释与测试 fixture）。
`[F]` **全仓库没有启动计时埋点**：非测试源码中 `performance.mark` 计数为 0（`grep -rl ... | grep -v tests` = 0）。因此"启动慢多少"必须外部实测，产品自身不提供数字。

---

## 3. 会话与流事件所有权（谁产生、谁折叠、谁序列化）

`[F]` 会话是**事件溯源**：append-only log + 内存 store + 派生 LLM 消息；持久化是插件而非服务本体（`packages/core/session/src/index.ts:1-5`）。
`[F]` `session/event` 是 **post-commit、fire-and-forget**：listener snapshot 在 log push 前解析，回调在 push 后执行，listener 抛错被吞（`packages/core/session/src/index.ts:70-84`）；`session/flush` 是"awaited parallel durability checkpoint"（同文件 :86-97）。

`[F]` 每个 token 的固定成本链（同一 chunk 被折叠 **3 次**）：
1. `packages/core/agent-loop/src/assistant-stream.ts:60-71`：`Date.now()` + `accumulator.push` + `assembler.push` + 立即 `emit({type:'chunk', revision, index, time, chunk})`；
2. `packages/api/session-controller/src/assistant-stream.ts:38-75`：host 侧**再建一个** `AssistantStreamAccumulator` 以支持断线重连 baseline，每帧 `push`；
3. `packages/api/session-controller/src/client/sessions/assistant-stream.ts:129-165`：浏览器侧第三次 fold，产出 transient entry（seq 用分数 `cursor+1-1/(n+1)` 插值）。

`[F]` 下行传输**没有批处理/合帧**：`packages/api/gateway/src/stream-server.ts:238-252` 是

```ts
for await (const value of source) { await this.send({ type: 'item', streamId, value }) }
```

即一个 item 一个 WebSocket 文本帧、串行 await。所有流复用一条 `/api/remote.mux`（`packages/api/gateway/src/stream-protocol.ts:11-19`），事件流是其中的 `$events` 伪 endpoint，帧类型 `ready/emit/waterfall/cancel`（同文件 :33-84）。

`[F]` 落盘：`persistBatch` → `appendLines`，**每个 batch 都 open → stat → writeFile → fsync → close**（`packages/session/session-persistence-jsonl/src/index.ts:1411-1435`），失败要 truncate 回滚。压缩是**拼接 zstd frame 容器**（`zstd.ts:1-20,48+`），并对小帧用 **Node 私有 zlib 内部**（`_handle.writeSync`、`_writeState`、symbol key）做同步解码优化（`zstd-private-decoder.ts:1-60,120-135`）——这是 JS 层为降低会话打开延迟而写的、与 Node 版本耦合的路径。

`[F]` 平台原语已经是 native 调用：Windows 用 koffi 绑 `MoveFileExW(MOVEFILE_WRITE_THROUGH)`（`win32.ts:1-16`）；POSIX 用 `flock(2)`、Windows 用命名 semaphore 做写所有权（`lease.ts:1-18`）。
`[F]` 会话列表/扫描用**协作式让步切片**避免阻塞事件循环：`packages/api/session-controller/src/list.ts:138-160`（`workSliceMs` + `performance.now()` 让出）。

---

## 4. Cordis/JS ↔ native 边界

`[F]` 仓库内**零 Rust**：`Cargo.toml` 0 个、`.rs` 0 个。native 只有预编译第三方件与一个小型 N-API 附加件：
- `node-pty`（ConPTY/pty，`packages/subprocess/subprocess-local/package.json` 依赖，`allowBuilds.node-pty` 显式允许）；
- `koffi` 3.1.1（FFI：`MoveFileExW`、LibreOffice 等）；
- `@deepseek-ai/node-addon-system`（Linux Landlock launcher + POSIX flock，`native/system/README.md`）；
- Electron 本体与 `@trycua/cua-driver`（computer-use，预编译）。

`[F]` **已经存在"native helper over JSONL stdio"范式**：computer-use 用一个外部 helper 子进程，按行 JSON 请求/取消，`NATIVE_TIMEOUT`/`NATIVE_CLEANUP` 等错误码（`packages/experimental/cua/src/native-client.ts:1-60,100-180`）。这是"局部 native 化"的既有先例，与"全量重写"是两条不同路线。

`[F]` **动态插件运行时要求必须能执行 JS**——这是全量 Rust 化的硬墙：
- host half：模型写的源码作为 async 函数体在 `node:vm` context 里执行，`createContext` + 自定义 traps + 双 realm `instanceof` 补丁（`packages/extensions/cordis-host-runner/src/sandbox.ts:105-127`），求值入口 `runInContext('(async () => {...})()', ...)`（同文件 :215-230）；define 时用 `new Function` 只编译不执行做语法门禁（:171-200）。
- client half：浏览器里 `new Function(...)` 求值（`packages/extensions/cordis-client-runner/src/client/evaluator.ts:180`）。
- 这类包由 `tool-cordis` 暴露给模型（`packages/extensions/tool-cordis`），即"模型可以在运行时给正在运行的产品加插件"。Rust 实现要保住该能力，必须内嵌 JS 引擎（V8/QuickJS/Boa），或者把该能力降级——两者都不是性能优化，而是产品能力取舍。

---

## 5. 规模（`[M]`，仅统计 `packages/*/*/src` 与 `apps/*/src` 下的 `.ts/.tsx`）

| 区域 | 行数 |
|---|---|
| **合计** `packages/*/*/src` | **438,127**（2,551 文件） |
| `packages/client`（浏览器 UI/Cordis 客户端树） | 146,693 |
| `packages/extensions`（动态插件沙箱 + tool-cordis） | 22,883 |
| `packages/api`（Remote/BFF 控制器与 wire 契约） | 22,478 |
| `packages/session` | 18,185 |
| `packages/core`（agent/loop/tools/session） | 14,571 |
| `packages/llm` | 14,352 |
| `packages/boot`（profile/loader/plugin-manager） | 11,106 |
| `packages/host` | 4,298 |
| `packages/terminal` | 2,015 |
| `apps/desktop/src` | 8,149 |
| `apps/desktop-host/src` | 405 |
| 工作区 package 总数 | 342 |

会话格式/迁移专项：`session-format`(945) + `v0-to-v1`(2,869) + `v1-to-v2`(1,256) + `v2-to-v3`(815) + `v3-to-v4`(1,675) + `session-persistence-jsonl`(5,084) + `session-persistence`(629) = **13,273 行**，只为实现"能读写别人写的会话文件"。

---

## 6. 六条关键事实（可直接引用）

- **F1｜agent 不在 UI 进程里**：Host 是独立 Node 进程，渲染器通过 WS 直连 Host；Electron 只做壳/代理/更新/原生对话框。Rust 化的边界因此天然存在（可以只换 Host）。
- **F2｜每次启动 ~150 个 ESM 动态 import，无编译缓存/快照/SEA**，且启动链上还串着 profile 组装、包解析、provenance 校验、文件锁。这是"启动慢"的**结构性**来源，而非某段 JS 写得慢。
- **F3｜两棵 Cordis 树、两套模块系统**：host 用 vendored loader（ESM 动态 import），浏览器用懒加载 CJS 表 + `__DSH_BOOT__` 图（79 个 client 包，bundle 由 `/plugins` 路由在**首次 GET 时**合成并 immutable 缓存，`packages/client/modules/src/index.ts:1-22,182,700,1167`）。任何"只重写后端"的方案都必须原样提供该图、该路由与该协议。
- **F4｜流路径未合帧**：每 chunk 三次折叠 + 一帧一 WS 消息 + 串行 await；这是唯一"Rust 能显著改写算法"的在线热路径，但它的速率由模型 token 速率决定（数量级 10²/s），是否成为瓶颈必须实测。
- **F5｜持久化是 I/O 与格式契约的主战场**：每 batch open/write/fsync/close；拼接 zstd frame；Node 私有 zlib 同步解码；koffi `MoveFileExW`；flock/semaphore。**这里 JS 已经在跟平台细节搏斗**，也是最"非 JS 擅长"的部分。
- **F6｜模型可写 JS 插件是硬约束**：host-half 在 `node:vm` 执行，client-half 在浏览器 `new Function` 执行。要保能力就得内嵌 JS 引擎。

---

## 7. 待验证性能假设（每条给出验证方式；禁止把结构当结论）

现有可用的作者自设预算（`[F]`，直接用其 fixture 做基线，比自己造数据更可信）：
- `benchmarks/session-open/session-open.bench.ts:38-66`：**127,400 事件**（200 turns × 500 text + 125 reasoning deltas）下 `migrationOpen 220ms / read 8ms / sessionRestore 24ms / projection 14ms / firstOpenFirstHistory 220ms / reopenFirstHistory 48ms / reopenAgentResume 40ms`，长会话 retained heap 期望 26.1MB。
- `benchmarks/conversation-fold/…bench.client.ts:40`：大 fold 16ms 预算；`benchmarks/active-stream-reconnect/…bench.client.ts:10`：断线重建 baseline 预算 125ms。

- **H1（启动）**：桌面冷启动的 wall-clock 主要由"Host 进程 spawn + ~150 ESM import/parse + 首次 bundle 合成 + React 挂载"构成，Electron 自身占比次之。
  验证：外部计时三刀切开——`app.whenReady`→Host `ready` IPC、`ready`→首帧、首帧→可交互；同时 `NODE_DEBUG=module`/`--cpu-prof` 统计 import 次数与耗时。**反证实验（无需 Rust）**：先开 V8 compile cache（`module.enableCompileCache()`，当前完全未用）与预合成 `/plugins` combo bundle，看能吃掉多少；若吃掉大头，则"必须 Rust 才快"不成立。
- **H2（首屏）**：首屏主要成本是 `/plugins` 首次 GET 的 bundle 合成与传输，不是 JS 执行。
  验证：对每个 bundle 路由分别计时 TTFB 与 body 大小（`client/modules` 已有 rev 化 URL，可细分），对比预热后的第二次加载。
- **H3（流）**：单会话 token 流在当前实现下 CPU 余量充足；成本会随"并发会话 × 工具大输出"线性放大，阈值在几十条并发流而非单流。
  验证：合成 N 条并发 `$events` 流（复用 `active-stream-reconnect` 的 harness 思路），测端到端 delta 延迟与 host CPU；再对照把 `for await … await send` 改为小批量合帧后的差异。**注意：合帧是纯 JS 可做的改动**。
- **H4（会话打开）**：冷开 127k 事件会话的成本集中在 zstd 解码 + 严格校验 + v0→v4 迁移，而非 fold 逻辑；Rust 原生解码/校验有数量级空间。
  验证：用仓库自带 `session-open` fixture 做 phase 分解（其 report 已分 phases），拿 `read 8ms / sessionRestore 24ms` 当上限参照；再与"Rust 只读同一批字节"的 PoC 对比。若 PoC 只快在 read 段（8ms 量级），用户不可感知。
- **H5（内存）**：Rust 的收益主要来自**进程模型**（去掉 V8 堆、去掉每进程 JIT 代码段），不是算法；单会话 26.1MB retained 已经不高。
  验证：分进程记 peak RSS（Electron main / renderer / Host / pty），分别在"空载、开一个长会话、跑一个 turn"三种状态采样，再与"Host 换成 Rust 但保留 Electron+Chromium"的可行性估算对齐。
- **H6（平台摩擦）**：`zstd-private-decoder` 的私有 API、`list.ts` 的协作让步、koffi `MoveFileExW` 都是"JS 打不过平台"的证据，但单次开销是 µs~ms 级。
  验证：分别 microbench 这三处（同步解码 vs 异步流、10k 会话列表切片、每次 append 的 fsync 占比），只有当"单次 ≥ 数十 ms 且出现频次高"才值得 native 化。

---

## 8. Rust 全量重写不可省略的兼容性成本（清单式，按"省不掉"排序）

1. **浏览器侧整套 UI 与它的插件树**（`packages/client` 146,693 行、79 个 `dsh.client` 包）：保留 → 必须实现 `__DSH_BOOT__` 图 + `/plugins` bundle 路由 + 懒加载 CJS 语义；重写 → 等于重做产品前端。
2. **wire 契约**：HTTP 路由 + 认证 cookie/`authenticatedUrl` + `boot injections` + `/api/remote.mux` WS + `$events` 帧语义（`ready/emit/waterfall/cancel`）+ uplink 窗口/字节预算 + 心跳（`stream-server.ts:42-75,238-252`；`stream-protocol.ts`）。
3. **Cordis 语义**：服务注入、fiber 生命周期、`internal/plugin` 事件、YAML 补丁分层与 `!!js` 求值、`cordis.yml` 写回、plugin manager 的安装/启停（走 pnpm 子进程）、启动审计与 fail-loud。
4. **会话文件契约**：v0→v4 全部格式与迁移、严格 payload 校验、拼接 zstd frame、`MoveFileExW` 写通、flock/semaphore 写所有权、torn tail 修复、fork 前缀语义（13,273 行等价逻辑 + 跨版本互操作）。
5. **动态插件沙箱**：模型写 JS 的执行能力（host `node:vm` / client `new Function`）→ 必须内嵌 JS 引擎或主动降级能力。
6. **agent 面**：工具 schema 与 presentation（PTC/native 两模式）、approval/ask_user、skills、subagent、MCP/LSP、terminal/browser、computer-use、workflow、compaction 策略、title/telemetry。
7. **桌面面**：更新通道（含 mandatory update）、NSIS 打包、单实例、tray/快捷键/更新日志、崩溃报告、平台会话注入。
8. **可观测性/契约测试**：仓库用极重的 vitest/typert 生成式契约；任何替换实现都要重新建立等价验证（作者自设的 perf gate 只是其中一部分）。

> 决策边界：wire、Cordis、会话格式与 JS 插件执行是候选兼容成本。启动、会话打开、持久化及流处理是待测路径，不是已证实热点。编译缓存、预合成、流合帧与句柄复用只作为对照候选；是否适用及收益/风险比仍需实测。
