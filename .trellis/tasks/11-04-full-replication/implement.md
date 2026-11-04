# Implement：Romp 完整复刻

> 执行顺序：环境 → 外壳加固 → core → chat → models → panels → terminal/browser/tools → system/scheduler → parity/打包。
> 每个阶段结束跑一次全量门禁；失败留在当前阶段，不把问题带到下一阶段。

## P0 环境与基线（前置，当前阻塞）

- [ ] 安装 Rust：`rustup`（stable，MSVC toolchain）
- [ ] 安装 MSVC Build Tools：`Microsoft.VisualStudio.2022.BuildTools` + `VCTools` workload + Win10/11 SDK
- [ ] `bun install`（前端依赖）
- [ ] sidecar：`OMP_SIDECAR=D:/ompgui/resources/omp.exe`（dev cwd 是 `src-tauri/`，仓库根 `resources/` 不会命中）
- [ ] `cargo test --workspace`
- [ ] `bunx tsc --noEmit`
- [ ] `bunx vite build`
- [ ] `cargo run -p omp-agent --example probe -- D:/ompgui/resources/omp.exe`
- [ ] `bunx tauri dev` 起窗，`scripts/capture-window.ps1` 留证
- [ ] 记录基线：冷启/ready、USS（无 tab / 1 tab 空闲 / 1 回合 / 2 tab）、空闲 CPU、退出
- [ ] 把预算写入 `perf/budgets.json`（新建）与 `docs/handoff.md`

**Gate G0**：以上命令全绿，基线数字可重复。未过 G0 不开始功能阶段。

## P1 外壳与传输加固

- [ ] Phase 2 窗口烟测：发 "list files in crates"，确认流式文本 + 工具卡
- [ ] capabilities 验证（event/window/webview/path）
- [ ] 事件信封形状测试与新增通道测试
- [ ] sidecar 生命周期：聚焦 tab 保持 warm，失焦且租约全零才释放；释放开关先只观测
- [ ] `batch_tab` 改 per-tab batcher；`rpc:command-for-tab` 接 `SidecarPool`
- [ ] 未实现通道返回统一错误并记录

**Gate G1**：一次完整对话 + 释放/恢复（开关关闭时只验证启动/停止）+ 无 per-tab 进程泄漏。

## P2 `11-04-core-tabs-sessions`（22 行）

顺序建议：

1. `omp-session` 索引/缓存（sessions 52–58）
2. tab 生命周期/持久化/分屏（tabs 78–83）
3. 会话切换/重命名/删除/归档（sessions 53–55）
4. 会话树/branch/fork/handoff/share/import/workspace（sessions 58–60、107–111、133）
5. 会话信息/模型恢复/选择器（sessions 108、112、132）

**Gate G2**：`docs/parity.md` 中 tabs+sessions 22 行全 `done`；开/切 tab 不 spawn；切回已释放 tab 先出缓存 transcript；`--resume` 恢复通过；`cargo test --workspace`。

## P3 `11-04-chat-agents`（27 行）

1. 核心回路：prompt/流式/工具卡/agent_end（chat 9–17、22–24）
2. steer/follow-up/abort/queue（chat 10、11、23）
3. markdown/代码/KaTeX/Mermaid/附件/补全（chat 12–20）
4. 扩展 UI/审批/ask（chat 16、104、105、106）
5. agents：roster/page/hover/BTW/Hub（agents 1–3、101–103、105）

**Gate G3**：完整回合 + steer/follow-up/abort + 审批 + BTW + 子代理页；chat+agents 27 行 `done`。

## P4 `11-04-models-settings`（23 行）

1. 模型选择/切换/thinking/fast（models 33–35）
2. provider 管理/自定义/角色/用量（models 35–38、134）
3. schema 设置/GUI 偏好/审批/运行时开关（settings 61–64）
4. 管理页/Extensions/Inventory/Modes/Compare/Hotkeys/Onboarding（settings 65、117–122）
5. 主题与 i18n（themes 89–91、i18n 31–32）

**Gate G4**：provider 登录/自定义模型/角色/用量窗口可用；设置即时生效；三语与主题切换无回归；23 行 `done`。

## P5 `11-04-panels-files-stats`（16 行）

1. 抽屉 7 视图（panels 43–48、113）
2. 文件树/预览/@mention/Git（files 26–30、113）
3. Stats 10 路由/服务托管（stats 66–70）

**Gate G5**：Files/Diff/Overview/MCP 面板不依赖 agent 在线；stats 10 路由可打开；16 行 `done`。

## P6 `11-04-terminal-browser-tools`（23 行）

1. 终端 PTY 生命周期（terminal 85–88）
2. 浏览器面板/CDP/链接路由（browser 5–8）
3. 工具卡 registry 全覆盖（tools 92–100、129–131）
4. PR Center（13、14）、Memory（15）

**Gate G6**：终端可开/输入/resize/关闭；浏览器可开页/前进后退；工具卡无 Generic 兜底遗漏；PR/Memory 可用；23 行 `done`。

## P7 `11-04-system-scheduler-packaging`（22 行）

1. 菜单/托盘/深链/窗口几何/退出守卫（system 71–75、123、128）
2. 通知/诊断/工具面板/反馈/benchmark（system 76、124–127、129）
3. 定时任务 CRUD/运行/审阅/通知铃（scheduler 48–51、129）
4. Windows 打包：sidecar 解析、图标、NSIS、`resources/omp.exe` 映射、`omp://`（packaging 39–42、system 77）

**Gate G7**：托盘/菜单/深链可用；定时任务 headless 跑通；`bunx tauri build` 产出 NSIS；22 行 `done`。

## P7b `11-04-cross-platform-release`（post-1.0，延期）

macOS/Linux 的 sidecar、WebView、安装包、签名/CI；本轮只保留平台扩展点，不验收。启动条件：Windows 1.0 发布后，或用户明确提前。

## P8 收口与发布

- [ ] `docs/parity.md` 审计：133/133 `done`；11 条 `skip` 有原因；无 `todo/wip`
- [ ] 专项回归：完整回合、多 tab、释放/恢复矩阵、终端/浏览器、定时任务、更新
- [ ] 性能门禁：Windows 完整基线；无 per-tab sidecar
- [ ] renderer diff：仅 `main.tsx` / `index.html`
- [ ] Windows 安装包：NSIS；unsigned 可用
- [ ] 许可清单：MIT/AGPL/Apache 声明齐全
- [ ] CHANGELOG + handoff 更新
- [ ] 打 tag / 发布

## 验证命令（每阶段）

```bash
cargo test --workspace
bunx tsc --noEmit
bunx vite build
bunx tauri dev          # Windows 窗口烟测
bunx tauri build        # P7：NSIS
```

## 回滚点

- 每个子任务独立提交；失败只回退该域。
- sidecar 释放开关默认关闭；异常可即时停用。
- 不修改 `src/renderer`；无需 UI 回滚。
- 安装包保留上一版；不自动迁移用户数据。

## 执行前置

按 Trellis 规划门：本文件与 PRD/Design 需用户明确批准后才能 `task.py start` 和改代码。P0 是唯一可以在批准后立即执行的环境阶段。
