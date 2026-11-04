# PRD：Tauri 外壳 + `window.omp` shim

## 背景

`replica/recon.md` 已把上游 `oh-my-pi-gui` 拆完：22 屏 / 54 对话框 / 26 流程 / 28 实体 /
100 条功能（must 23）。核心回路 F01 = composer 提交 → RPC `prompt` → 流式 markdown + 工具卡 → `agent_end`。

Rust 侧传输层已落地并验证（`crates/omp-core` / `omp-rpc` / `omp-agent`，37 项单测通过；
`cargo run -p omp-agent --example probe` 对 fork sidecar 的 6 条命令全部 `success:true`）。
本任务把「没有 UI」这一步补上：用 Tauri 2 起窗，让**原样复用**的 `src/renderer` 跑在 WebView2 里。

## 范围

1. `src-tauri/`：`Cargo.toml`、`build.rs`、`tauri.conf.json`、`src/main.rs`、`src/lib.rs`、`src/ipc.rs`。
2. IPC：只注册一个命令 `ipc_invoke(channel, payload)`，内部 match `IPC_COMMANDS` 全表；
   事件用 `emit`/`listen`，通道名取自 `IPC_EVENTS`。
3. `src/bridge/omp-api.ts`：`createOmpApi()` 把 21 个命名空间映射到 `invoke`/`listen`。
4. 前端工程：`package.json`（上游版本区间，删 Electron）、`vite.config.ts`、`tsconfig.json`、
   `src/renderer/index.html`（Vite 入口 + 新 CSP）、`public/pre-paint.js`。
5. 源补齐：`src/renderer` + `src/shared` 从 `.research/omp-gui-git`（v0.9.35）拷入，字节级一致。

## 不在范围

- 会话索引、多 tab、统计、定时任务、终端、浏览器（Phase 3–8）。
- 打包（Phase 9）。本任务只要求 `cargo tauri dev` 能起窗并跑通一次对话。

## 验收标准

- [ ] `cargo tauri dev` 起窗，标题栏/会话列表/输入框可见（S01 shell + S02 chat stream）。
- [ ] 发一条 "list files in crates" → 出现流式 markdown + 至少一个工具卡（F01、F02）。
- [ ] `bunx tsc --noEmit` 无 `Cannot find module './…'`。
- [ ] `src/renderer` 与上游 v0.9.35 逐文件 diff 仅允许 `main.tsx`（加 `installOmpApi()`）与 `index.html`。

## 证据

- 屏幕/流程：`replica/parts/screens.md`（S01、S02、D01–D04）、`replica/parts/flows.md`（F01、F02）。
- IPC 契约：`.research/omp-gui-git/src/shared/ipc-types.ts` 的 `IPC_COMMANDS` / `IPC_EVENTS` / `OmpApi`。
- 传输层行为：`crates/omp-rpc/src/{frame,client,transport}.rs`。
