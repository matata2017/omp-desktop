# Design：Romp 完整复刻

## 1. 架构基线

以 `docs/architecture.md` 为准：

- `src-tauri` 只做 IPC/窗口/插件适配，业务在 Rust 域 crate。
- `src/renderer` / `src/shared` 保持上游 v0.9.35 字节级复用，差异仅 `main.tsx` / `index.html`。
- agent 是子进程：活动会话启动 `omp --mode rpc`；失焦/空闲且租约归零后释放；再次激活用 `--resume` 恢复。
- tab 是视图：打开/切换先读 `omp-session` 缓存；composer 等 sidecar `ready` 后解禁（现有 renderer 门禁）。
- 进程数 = 活动回合数 + 至多 1 个空闲预热槽；默认并发回合上限 2。

## 2. 任务拓扑与依赖

```
11-04-full-replication (program)
├─ 11-04-core-tabs-sessions      tabs + sessions                  22
├─ 11-04-chat-agents             chat + agents                    27
├─ 11-04-models-settings         models + settings + themes+i18n  23
├─ 11-04-panels-files-stats      panels + files + stats           16
├─ 11-04-terminal-browser-tools  terminal+browser+tools+pr+memory 23
├─ 11-04-system-scheduler-packaging system+scheduler+packaging    22
└─ 11-04-cross-platform-release  macOS/Linux（post-1.0，延期）     0
```

依赖：

- core-tabs-sessions 是其余域的前置（tab 生命周期、sidecar 归属、事件路由）。
- chat-agents 依赖 core 的 transcript 缓存与 agent 进程管理。
- panels-files-stats、models-settings、terminal-browser-tools 可并行，但都复用 core 的 IPC/事件契约。
- system-scheduler-packaging 最后收口：托盘/菜单/更新/NSIS 依赖功能表面稳定；cross-platform-release 为 post-1.0 延期项。
- 父任务只做跨域契约、验收汇总、parity 审计；不直接实现功能。

## 3. 共享契约

1. **IPC**：`ipc_invoke(channel, payload)` 不变；channel 名以 `src/shared/ipc-types.ts` 的 `IPC_COMMANDS` 为准；新 Rust 命令用 `tauri-specta` 生成类型，但 renderer 侧仍走现有 shim。
2. **事件**：两种信封形状（`{tabId,payload}` / `{request,tabId}`）由 `src-tauri/src/lib.rs` 的 `event_payload()` 与单测锁定；新增事件先加测试。
3. **sidecar argv**：能力探测决定 `--chat/--no-auto-resume/--session/--no-open`；绝不向 stock 传未知 flag（`crates/omp-agent/src/argv.rs`）。
4. **超时**：`RPC_COMMAND_TIMEOUTS` 原样透传；新命令按上游表补。
5. **状态账本**：每完成一条功能，在 `docs/parity.md` 对应行改 `done` 并附证据（测试名/烟测步骤/截图）；`replica/features.csv` 是条目源，不改行数除非新增侦察。
6. **renderer 守则**：不修改 `src/renderer`；Rust/bridge 适配现有状态机（例如 `status === "ready"`）。
7. **许可**：新增依赖/资源进 `LICENSES/` 或 `THIRD_PARTY` 清单；发布前审计。

## 4. 验证策略

每条功能行的 Definition of Done：

1. **实现**：载体符合 `clone` 列；无 TODO/占位。
2. **自动验证**：Rust 单测/集成测试或 renderer 现有 vitest；新增契约测试只在行为有风险时写。
3. **烟测**：可观察的 UI/命令行为；涉及窗口的用 `scripts/capture-window.ps1` 留证。
4. **账本**：`docs/parity.md` 状态与证据同步。
5. **回归**：`cargo test --workspace`、`bunx tsc --noEmit`、`bunx vite build` 全绿；renderer diff 仍仅两文件；Windows `tauri dev`/`tauri build` 通过。

域级验收：

- core：开 tab 不 spawn；激活后 ready；失焦/空闲释放；`--resume` 恢复 transcript；并发上限生效。
- chat：完整回合（prompt → 流式 → 工具卡 → agent_end）；steer/follow-up/abort；审批与扩展 UI。
- models/settings：provider 登录/自定义/角色/用量；schema 设置即时生效；三语与主题。
- panels/files/stats：抽屉 7 视图；文件树/预览；Git 变更；stats 10 路由。
- terminal/browser/tools：PTY 生命周期；浏览器 CDP；工具卡 registry；PR/Memory。
- system/scheduler/packaging：菜单/托盘/深链/更新；定时任务 headless；NSIS 安装包。
- cross-platform：post-1.0；当前只保留平台扩展点，不做构建/签名/CI。

## 5. 关键权衡

| 决策 | 取舍 |
|---|---|
| 保留上游 renderer | 不能改 UI 状态机；换来 567 文件零维护、功能对齐快 |
| 不重写 agent | 保留 244MB sidecar；用生命周期/释放换内存，而不是重写 |
| Rust 域 crate 渐进迁移 | 早期仍有 sidecar 依赖；按域迁移，避免大爆炸 |
| 释放默认关闭 | 先观测租约，再开恢复；牺牲短期内存收益换数据安全 |
| Windows 优先 | 当前只交付 Windows x64；macOS/Linux post-1.0，业务层避免 Windows-only API，保留迁移缝隙 |

## 6. 回滚

- 每个子任务独立分支/提交；失败按域回退，不影响其他域。
- 释放开关是 feature flag，可即时关闭。
- 不修改上游 renderer；不需要回滚 UI。
- parity 账本只前进不删除；`degraded` 必须写原因。
- 打包/更新失败保留上一版安装包；不自动迁移用户数据（`prefs.json` 新结构）。
