# PRD：Romp 完整复刻程序

## 1. 目标

把上游 `nornzach/oh-my-pi-gui` v0.9.35 的全部功能复刻到 Rust/Tauri 2 应用 `Romp`，按功能分阶段交付，最终在 `docs/parity.md` 中 133 条功能全部达到 `done`（或按 skip 规则明确排除），并通过构建、烟测与性能预算。

用户价值：得到与上游功能等价、但常驻内存与冷启显著更低的桌面应用；可以逐块验证，不需要一次性重写。

## 2. 已确认事实

- 上游：`nornzach/oh-my-pi-gui` v0.9.35（MIT），renderer 567 文件，`src/renderer` / `src/shared` 已按 tag 原样拷入；字节级差异仅 `main.tsx`（`installOmpApi()`）与 `index.html`（CSP/入口）。
- 功能账本：`replica/features.csv` 133 行，priority must 23 / should 89 / could 21；`replica/parts/features.md` 为人类可读版；`replica/parts/screens.md` 22 屏 + 54 对话框（76 个可见表面）；`flows.md` 26 流程；`data-model.md` 28 实体 + 197 条 RPC 命令。
- 状态账本：`docs/parity.md` 逐条记录 `todo/wip/done/degraded/skip`；机器可读源是 `features.csv`。
- 现有实现：Phase 0–2 代码完成（`crates/omp-core` / `omp-rpc` / `omp-agent`、`src-tauri`、`src/bridge`）；文档记录 59 项 Rust 测试与协议探针通过，但本机当前无 `cargo/rustc/MSVC/node_modules/resources`，无法复现。
- 架构决定：`docs/architecture.md`（Rust 常驻服务 + 可回收 agent；活动会话启动 sidecar、失焦/空闲释放；tab 不各自持有进程）。
- 许可：renderer 为 MIT（`LICENSES/oh-my-pi-gui-MIT.txt`）；Trellis 为 AGPL-3.0；本工程 MIT。

## 3. 范围

### 3.1 功能范围

全部 133 条功能行，按 6 个功能交付域拆分；macOS/Linux 发布域延期到 post-1.0：

| 子任务 | area | 行数 |
|---|---|---:|
| `11-04-core-tabs-sessions` | tabs 7 + sessions 15 | 22 |
| `11-04-chat-agents` | chat 20 + agents 7 | 27 |
| `11-04-models-settings` | models 7 + settings 11 + themes 3 + i18n 2 | 23 |
| `11-04-panels-files-stats` | panels 6 + files 5 + stats 5 | 16 |
| `11-04-system-scheduler-packaging` | system 13 + scheduler 5 + packaging 4 | 22 |
| `11-04-terminal-browser-tools` | terminal 4 + browser 4 + tools 12 + pr 2 + memory 1 | 23 |
| `11-04-cross-platform-release` | macOS/Linux 构建/签名/CI | 0（延期） |

每行必须：实现载体符合 `clone` 列（reuse/rust/plugin/hybrid）；有可观察的验证证据；在 `docs/parity.md` 中从 `todo` 推进到 `done`（不能验证的标 `degraded` 并写原因）。

### 3.2 平台范围

**当前交付 Windows x64**：NSIS 安装包，`Romp.exe` + `resources/omp.exe` sidecar。macOS/Linux 延期到 post-1.0，但架构与代码不得写死 Windows：平台相关路径集中在 Rust 适配层，Tauri target、sidecar 解析、WebView/CDP、托盘/菜单保留扩展点。签名/公证随对应平台一起后置。

## 4. 验收标准

- **A1 功能账本**：`docs/parity.md` 中 133 条功能全部为 `done`；`skip` 仅限已记录的 11 条，且每条有原因。没有 `todo`/`wip` 残留（除明确 deferred）。
- **A2 子任务验收**：6 个当前子任务的 PRD 验收全部通过；`11-04-cross-platform-release` 为 post-1.0 延期项。
- **A3 构建与测试**：
  - `cargo test --workspace` 通过；
  - `bunx tsc --noEmit` 0 error；
  - `bunx vite build` 成功；
  - `bunx tauri dev` 在 Windows 起窗，完成“发 prompt → 流式回复 → 工具卡 → agent_end”烟测；
  - `bunx tauri build` 产出 NSIS 安装包（Windows x64）。
- **A4 性能基线**：Windows 记录冷启、ready、USS（无 tab / 1 tab 空闲 / 1 回合 / 2 tab）、空闲 CPU、退出耗时；预算写入 `perf/budgets.json` 并由 CI/脚本门禁。macOS/Linux 基线随后置平台补测。
- **A5 renderer 一致性**：`src/renderer` 对 tag v0.9.35 的差异仍仅 `main.tsx` / `index.html`；新增改动不进入 renderer。
- **A6 许可**：发布物附带 MIT/AGPL/Apache 三方声明（按实际依赖补全）。

## 5. 不做什么

- 不重写 omp agent 为 Rust；agent 仍是 sidecar 子进程（见架构文档）。
- 不替换 React renderer / 不换纯 Rust UI。
- 当前不做 macOS/Linux 的构建、安装包、签名/公证与 CI；保留架构扩展点，post-1.0 由 `11-04-cross-platform-release` 接手。
- 不做移动端。
- 不在本轮做 ACP 替换现有 RPC 数据层（只作为未来缝）。

## 6. 依赖与风险

| 风险 | 影响 | 缓解 |
|---|---|---|
| 本机无 Rust/MSVC/node_modules | 所有实现无法编译/烟测 | M0 先装工具链与依赖；未就绪前只写计划，不宣称完成 |
| sidecar 244MB、per-tab 进程 | 内存与目标相反 | 按架构文档：活动会话启动、失焦/空闲释放、全局并发上限 |
| 释放后终端/浏览器/MCP 不可恢复 | 数据丢失 | 先迁资源到 Rust，再加租约；恢复矩阵通过前默认关闭释放 |
| renderer 字节级复用约束 | 不能改 UI 侧状态机 | Rust 适配现有 `status === ready` 门禁，激活时启动 |
| 平台相关代码写死 Windows | post-1.0 迁移成本高 | 平台差异集中在 `src-tauri` 适配层与 `omp-agent` 的 sidecar 解析；业务 crate 不引 Windows-only API |
| Windows 未签名安装包 | SmartScreen 提示 | 先交付 unsigned；有证书后补签，不影响功能验收 |
| macOS/Linux 需求提前 | 分散当前交付 | 明确 post-1.0；只保留扩展点，不投入构建/烟测 |
| 上游/依赖许可 | 发布合规 | 发布前跑许可清单与 `LICENSES/` 对照 |

## 7. 开放问题

无阻塞问题。当前范围是 Windows x64；macOS/Linux 与签名/公证延期到 post-1.0，不影响本轮功能与构建验收。
