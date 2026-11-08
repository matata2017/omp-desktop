# Journal - ompdev (Part 1)

> AI development session journal
> Started: 2026-11-04

---

## 2026-11-04 — Romp 0.1.0-dev 快照（tag `2026-11-04`）

**完成**

- P0 工具链：Rust 1.99.1 + MSVC 14.44.35207 + Win11 SDK 26100 + bun 依赖；`scripts/dev.ps1` 一键起窗（注入 MSVC/PATH/`OMP_SIDECAR` + WebView2 CDP 9222）。
- Phase 1/2：Rust workspace（omp-core / omp-rpc / omp-agent / omp-session）、单一 `ipc_invoke`、per-tab client 与事件合批、窗口 ready、完整 prompt→流式→工具卡→agent_end。
- 侦察与账本：`replica/` 22 屏 / 54 对话框 / 26 流程 / 28 实体 / **133** 条功能（must 23 / should 89 / could 21）；`docs/parity.md` 逐条状态。
- 会话域第一版：索引（32 KiB 头尾窗口 + `(size,mtime)` 缓存 + family 折叠）、`sessions:list/delete/rename/search`。
- 会话切换改为**常驻 sidecar 原地切**（对标 `openai/codex` app-server 的 `thread/resume`）：同 workspace 直接 `switch_session`，不再每次新起 244 MB 进程；补发 `tab:status` 快照；veto/未确认的切换不认领归属。
- 终端帧路由：`terminal_opened/output/exit/closed` → `terminal:frame`（`{tabId,frame}`），实测收到 output 帧。

**验证**：`cargo test --workspace` 65 passed；`bun run test` 186 files / 1571 passed；`tsc` 0 error；协议探针 6/6；`terminal_open` 实测有帧。

**未解决（下一位接手先看）**

1. phantom tab：启动后偶发无人点击的 tab，来源未定位 → 所有 UI 测量前先加 `tab:spawn` 来源日志。
2. fresh tab 会话：argv 已带 `--no-auto-resume`，但一次 `get_state` 仍报旧会话，根因未确认。
3. 终端面板 UI、浏览器内置 webview host、`scheduler:*`/`models:*`/`updater:*` 等通道未做。

**交接**：`docs/handoff.md`（当天完整状态、运行方式、风险、下一步顺序）。


## Session 1: Perf baseline and deep measurement
<!-- trellis-session: v=2 fp=1a8108639484f25b -->

**Date**: 2026-11-08
**Task**: Perf baseline and deep measurement
**Package**: omp-agent
**Branch**: `master`

### Summary

Merged the perf harness, measured real release builds (1 tab 712 MB, 4 tab 1442.7 MB), split boot into four milestones, compared against upstream Electron, vetoed shared sidecar, profiled streaming, ran UI copy/axe audit, made stats lazy, and added a regression gate. 3 commits; cargo test 158, vitest 1571, typecheck all pass.

### Git Commits

| Hash | Message |
|------|---------|
| `a4d1e6a` | chore(trellis): perf baseline task artifacts and measurement spec guide |

### Status

[OK] **Completed**
