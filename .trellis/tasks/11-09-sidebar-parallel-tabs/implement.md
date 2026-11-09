# 实施计划: 侧边栏会话并行打开

## 前置（已完成）

- 用户已批准 planning summary；`task.py start 11-09-sidebar-parallel-tabs` 已执行，`status: planning → in_progress`（degraded mode：无 session identity 但照样翻 status、跑 hooks、返回 0；`current --source` 为 `(none)` 不影响 task 状态）。**不要手工改 `task.json`**。
- start 警告 `branch=master == base_branch`，archive 会拒；收尾前按需 `task.py set-branch <task> <feature-branch>`。
- `task.py validate` 通过，jsonl 上下文非空。
- 工作区基线：`master` == `origin/master`（a5849c2）；本地 stash@{0} 与 `backup/master-before-remote-reset-2026-11-09` 保留，本任务不触碰。

## 有序清单（1–5 已完成）

1. 加载上下文：inline 模式读 `trellis-before-dev` 与 `.trellis/spec/guides/cross-layer-thinking-guide.md`。
2. ✅ 改 `src/renderer/hooks/use-session-switch.ts` `canBrowseInPlace`：cwd 守卫后加

   ```ts
   const messages = useMessagesStore.getState();
   if (tab.sessionPath || messages.messages.length > 0 || messages.liveMessages.length > 0) return false;
   ```

   并同步更新 `:241-246` 与 `:276-281` 注释。`useMessagesStore` 已在 `:29` import，无需新增。**不使用** `messageCount > 0`、不单独用 `tab.sessionPath`、不让 Rust 补发字段、不用 `sessionRuntimeStore`（测试 harness 无 runtime）。
3. ✅ 改 `src/renderer/hooks/use-session-switch.test.ts`：
   - `seedFocusedTab(blank=false)` **不伪造 `sessionPath`**，改为给 `useMessagesStore` 灌一条历史消息；主用例断言 `tabs[0].sessionPath === undefined`（真实恢复态）。
   - 重写原 `:508-529` 为「已承载会话的 tab 点两次 → 2 次 spawn、无 `switchSession`、store 2 个 tab」。
   - 新增 `opens a new tab when only unflushed live rows remain`。
   - 保留 `:531-600` 全部守卫用例。
4. ✅ 按 design.md §6 更新 8 处文档；共享 sidecar 否决结论与「串行复用不是并发共享」原句保留。
5. ✅ 验证门（见下）。
6. **交付门 A8**：重建 `dist` + NSIS → 静默安装 → 在安装版重放并行冒烟。
7. `trellis-check` → `trellis-update-spec` → 提交 → `/trellis:finish-work`。

## 验证门（已跑）

```bash
bunx vitest run src/renderer/hooks/use-session-switch.test.ts   # 26 passed
bun run typecheck                                                # tsc --noEmit, exit 0
bun run test                                                     # 187 files / 1590 passed
cargo test --workspace                                           # Rust 未改，回归门；走 scripts\with-msvc.cmd
```

前端三门已绿（`GATE_EXIT=0`）。Rust 零改动，`cargo test --workspace` 只作回归确认，待跑。

## 交付门 A8（用户实际运行的版本）

用户此刻跑的是 `%LOCALAPPDATA%\Romp\romp-desktop.exe` 安装版，`dist` 是打进安装目录的资源。**只改源码 / 只跑单测不算完成**：

```bash
MSYS2_ARG_CONV_EXCL='*' cmd.exe /c "scripts\with-msvc.cmd bunx tauri build"
# target/release/bundle/nsis/Romp_0.1.0_x64-setup.exe
# 静默安装（currentUser，无需 UAC）：Start-Process <setup> -ArgumentList '/S' -Wait -PassThru
```

安装前**正常退出**已运行的应用，不要 `Stop-Process` 强杀（`docs/handoff.md:252`：强杀主进程会留下 sidecar 孤儿）。`scripts/dev.ps1` 见到 `romp-desktop` 会 throw，且 single-instance 会让带 CDP 参数重启的安装版只聚焦旧窗口后退出；dev 模式冒烟只能作为开发期辅助证据。

## 并行冒烟（安装版，A6）

正常退出 → 重装 → 启动安装版，点同 workspace 两个会话，观察：

- `tab:spawn` 出现 2 次、`spawning sidecar` 出现 2 次、`omp.exe` 进程 2 个；
- 两个 tab 各自 `sessionPath` 正确；A 起 prompt 后点 B，**A 的流继续到 `agent_end`**、B 正常 ready，互不打断；
- 校验安装目录 `omp.exe` 哈希仍与 `resources/omp.exe` 一致。

**不得**用「日志无 `switch_session`」当不变量：`ipc.rs` 只 debug 打印 channel 名，`openTab` 的 `pendingSessionPath` 在 ready 后会由 `tabs.ts:1072-1073` 合法补发 `switch_session`。

安装版没有 CDP（9222 只在 `scripts/dev.ps1` 注入）；如需 CDP，用 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` 启动安装版，或退而用 `runtime.log` + 进程表做证据。

## 风险文件与回滚点

- `src/renderer/hooks/use-session-switch.ts`：唯一产品代码改动，主回滚点。
- `src/renderer/hooks/use-session-switch.test.ts`：契约测试；原 `:508-529` 是既有实现测试，已改写而非重钉旧行为。
- 8 处文档：已逐条核对，未删 `probe-shared.mjs` 的否决结论。
- 不触碰 `src-tauri/`、`crates/`、`resources/omp.exe`、`tui_open` 相关代码。

## 完成前检查

- [x] planning summary 已获用户明确批准
- [x] `task.py start` 已执行、status=in_progress（degraded）
- [x] `task.py validate` 通过
- [x] 前端三个验证门全绿
- [ ] `cargo test --workspace` 回归门
- [ ] A8 交付门：安装版重建 + 静默安装 + 并行冒烟
- [ ] `set-branch` 处理 master==base_branch 的 archive 冲突
- [ ] 无其它未提交改动混入本任务

## 执行期扩大范围（用户批准：一并查修 F-OWN）

- 核查 `runtime.log` 01:43–01:53 + `tab-layout.json`：**F-OWN 未失效**。`tab-2` 由 `sync_session_binding` 合法迁移到另一会话并释放旧归属，`tab-3` 才打开它；三个 tab 绑三个不同 `.jsonl`，三个 `omp.exe` 并存。「两个 tab 抢同一会话」列为待验证假设，未作为根因。
- 确认缺陷：`tab:spawn` 的 check-then-claim TOCTOU（owner 检查 `:155` / claim `:187` 之间隔着 `start_tab`，且 `let _ =` 吞掉 claim 错误）。
- ✅ 修复 `src-tauri/src/ipc.rs`：分配 `TabId` 后、`start_tab` 前原子 `claim_session`，失败返回 `refusal:"owned"`，`start_tab` 失败回滚 claim 与三个 map。不改 `release_process` 保留归属的语义。
- ✅ 新增 `crates/omp-agent/src/pool.rs` 测试 `releasing_a_process_keeps_the_session_binding_for_resume`。
- ✅ `cargo test --workspace` 重跑：179 passed（原 178 + 新增 1），`CARGO_EXIT=0`。
- ✅ 最终 NSIS 重建含 renderer + Rust 两处修复。

## A8 交付门实测证据（安装版，2026-11-09）

- **产物核对**：安装目录 exe 内嵌新 dist 资源名 `index-BxxRkkvB.js`，旧名 `index-vlM-Bzrf.js` / `index-uhY0KJOy.js` 均不存在。与 `target/release/romp-desktop.exe` 逐字节仅差 3 字节（offset 17019726，`NSS` vs `UNK`，Tauri NSIS bundle-type patch）。`OMP_EXE_MATCH=True`。
  - 注：同版本 NSIS `/S` 会跳过覆盖，先 `uninstall.exe /S` 再装；`INSTALL_EXIT=0` 单独不足以证明写入。
- **真实入口冒烟**：CDP 点击 `.omp-sidebar-session-row`（`onClick` → `openSession` → `openSessionFromSidebar`），目标为 `owner:null` 的同 workspace 会话 `01a1afa6…`；active tab 为有历史的 tab-5。结果 **tab 数 5 → 6**，`NEW_TAB_OPENED=true`。旧代码在此会原地切、tab 数不变。
- **日志与进程**：`tab:spawn tab=tab-6` → `spawning sidecar tab=tab-6 binary=C:\Users\Admin\AppData\Local\Romp\omp.exe` → `sidecar ready tab=tab-6`；**3 个安装版 sidecar 并发**（PID 1204 / 10248 / 44348），各绑不同会话。
- **F-OWN 回归**：对已被 tab-5 占用的 `01a1bdd9…` 重复 spawn → `{tabId:null, refusal:"owned", ownerTabId:"tab-5"}`，tab 数 6 → 6 不变（`REFUSED_OWNED=true`、`NO_EXTRA_TAB=true`）。
- **优雅退出**：`CloseMainWindow()` 后主进程与其 sidecar 全部退出；机器上另有 9 个 `omp.exe` 属 `D:\ompgui`（上游 Electron GUI，父进程 `XZDesktop64.exe`），与 Romp 无关，未触碰。
- 临时 CDP 脚本已删除。

## 收尾

- spec 新增 `.trellis/spec/guides/session-tab-ownership.md` 并登记进 `guides/index.md`：claim-before-spawn、空白 tab 判据取舍、交付到安装版、共享机器上的进程计数口径。
- `branch` 元数据保持 `master`（项目直接提交 master，不引入不存在的分支名）。

### 前置条件隔离（补验，回应 review）

`NEW_TAB_OPENED=true` 单独不能归因到内容守卫——空白 tab 误走新开路径也会得到同样结果。补测 tab-5（点击时的 active tab）：

- `get_state` → `sessionFile=…01a1bdd9….jsonl`、**`messageCount=438`**、`isStreaming=false`、`isCompacting=false`、`queuedMessageCount=0`。
- `get_transcript_page(limit:5)` → **`rows=5`**（`messages`/`nextCursor`/`totalMessages`），即真实 hydrated 历史，tab-5 明确非空白。
- 点击后 `getSessionOwner(…01a1bdd9…) → tab-5`（原会话未被切走）、`getSessionOwner(…01a1afa6…) → tab-6`（被点会话归新 tab）。
- `tab:spawn tab=tab-6` 日志的 `session=` 正是被点击的 `01a1afa6…`，+1 可归因到本次点击。

结论：streaming / compacting / queue 三个守卫均不成立且 tab 非空白，「空白 tab misfire」被排除。未捕获点击瞬间的 composer 草稿与 extension-UI 状态，故不宣称内容守卫是唯一原因；但被质疑的 misfire 场景已否定。

`tab-4` / `tab-5` 日志里同 `resume=01a1bdd9` 已定性为归属漂移而非双重绑定：`claim_session` 是单锁临界区（`pool.rs:267-278`）且 spawn 前 claim，`getSessionOwner` 实测该会话唯一归属 tab-5。
