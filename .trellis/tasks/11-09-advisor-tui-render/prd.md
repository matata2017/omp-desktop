# PRD: advisor 配置 TUI 渲染错乱

## Goal

点击 advisor 眼睛打开的终端 TUI 边框散成竖列、内容挤压。修到**用户实际运行的安装版**上恢复正常等宽布局。

## 根因（完整因果链，每环有实测证据）

1. `src-tauri/tauri.conf.json:25` 声明 `style-src 'self' 'unsafe-inline'`。
2. Tauri 编译时解析前端资产并注入 nonce，有效**响应头**变成 `style-src 'self' 'unsafe-inline' 'nonce-<random>'`（实测取到）。
3. **CSP 规范：directive 中存在 nonce/hash 时 `'unsafe-inline'` 被忽略**。
4. xterm `DomRenderer._injectCss`（vendor sourcemap `:156-159`）在 `.xterm-screen` 内创建 `<style>`，**不带 nonce**。
5. 被 CSP 拦下 → `style.sheet === null`。实测同文档内：Tauri 自己那张带 nonce 的 `hasSheet:true`；xterm 两张（105 / 54823 字符）`nonce:null, hasSheet:false`。
6. `.xterm-rows` 拿不到 `white-space:pre` / mono / 12px，继承 body 的 `normal` / sans / 15px。
7. 空格折叠 → 边框左移 → 空行视觉堆成 `│ │ │`。

**数据链路全程正确**：sidecar 原始流 289 个 CSI；`terminal_resize` 确实发出且 sidecar `HONORS_RESIZE`；xterm buffer `maxLen/nonEmpty/boxRows` 与 sidecar 直出网格逐项吻合。坏的只有最后一步样式应用。

## Requirements

- R1 `src-tauri/tauri.conf.json` 的 `app.security` 增加 `"dangerousDisableAssetCspModification": ["style-src"]`，只阻止 Tauri 往 **style-src** 注入 nonce/hash，恢复配置已声明的 `'unsafe-inline'`。
- R2 **`script-src` 的 hash 保护不得改动**。
- R3 不改 `src/renderer` 任何文件（`TerminalPanel.tsx` / `components.css` 不在 `docs/handoff.md:371` 字节级白名单内，本方案绕开该约束）。
- R4 不改 sidecar、不改终端 RPC、不改 xterm 配置。

## Acceptance Criteria

- A1 **裁判先红**：修复前在安装版跑宽度裁判必须 false。已实测：独立参照 `cellWidth = rowsWidth 518 / ptyCols 72 = 7.194`；盒线列 `[0,27,71]` 实测 x `[0, 94.89, 163.5]`，期望 `[0, 194.25, 510.81]`，**残差 `[0, −99.36, −347.31]`** → `WIDTH_MATCHES=false`。
- A2 **裁判转绿**：重建 + 静默安装后，在**安装版**跑同一裁判，`WIDTH_MATCHES=true`，每根盒线残差 ≤ `max(1.5, cell×0.25)`。
- A3 **机制辅助**：`.xterm-screen style` 两张 `hasSheet:true`；`white-space:pre`、monospace、12px。辅助项不单独作为通过条件。
- A4 **重新测量**：字体由 15px sans 变 12px mono 会改变 cell 宽度与 `fit()` 得出的 cols，**不得沿用 72 / 7.194**，必须现场重取 `ptyCols` 与 `rowsWidth`。
- A5 **落地 + 生效的主判据**：安装后在安装版 `fetch(location.href)` 读**有效响应头**，断言 `style-src` **不再含 `'nonce-`**（坏状态含 `'nonce-2043626238886117981'`），且 `script-src` 仍含 sha256 列表。这一条同时证明新二进制已落地与修复生效。
- A6 交付门：`bunx tauri build` → **卸载后**静默安装 → 安装版复现（同版本 `/S` 会跳过覆盖）。
- A7 **更宽的修复面冒烟**：`style-src` 是文档级的，凡运行时注入 `<style>` 的都被同一条 CSP 拦过。已在出厂 bundle 核实：`codemirror-DH2DLpYI.js` 有 `createElement("style")`，`mermaid-MrnEQlGY.js` 有两处且无 `adoptedStyleSheets` 分支。安装后除终端外，**另冒烟 CodeMirror 面**——源码中引用 CodeMirror 的只有 `ComposerEditorDialog.tsx:9-11`（Ctrl+G 全屏草稿编辑器，双 Esc 丢弃、无持久副作用）与 `ExtensionDialog.tsx:14-15`；**`TaskEditorDialog` 不含 CodeMirror**，走定时任务那条路永远量不到 `.cm-editor`。mermaid 面若未挂载则显式记为未执行，不得写成已通过。

## 交付验证法的失效与替换

本次只改 `tauri.conf.json`，renderer 源码未动，`dist/assets/index-BxxRkkvB.js` **与前两次构建同名**。因此「查内嵌 dist 资源名是否变化」这条旧验证法在此**失效**，据此会误判「没装上」。改用 A5 的响应头判据——它更直接、更硬，且同时覆盖落地与生效两件事。

## 影响面（须写进 handoff，不能只记成终端修复）

修复恢复的是**文档级内联样式能力**，受益面至少包括：xterm 终端、CodeMirror 编辑器、mermaid/cytoscape 图表。这是超出「advisor TUI」的可见行为变化，属预期收益，但必须如实登记。

## 裁判设计（不 sound 的初版已废弃）

初版用 `whiteSpace==='pre' && fontFamily 含 monospace`——硬加两条 CSS 即可假绿。
第二版用两根盒线反推 cellWidth——两点定直线，残差恒 0，**可证必然通过**。

**现用**：cellWidth 取自**独立参照** `rowsEl.getBoundingClientRect().width / terminal_list.cols`；选 **≥3 根 `│` 且跨列 >20** 的行；用 `Range.getBoundingClientRect()` 量每根的实际 x。已在坏状态实测判红。

探针另修两处取样缺陷：等内容真正到达（轮询至 `nonEmpty>=5`）而非固定 sleep；CDP target 显式匹配 `tauri.localhost` 而非 `!startsWith("http")` 兜底。

## Out of Scope

- 不改 xterm 版本、不新增依赖、不引入 `@xterm/headless`。
- 不动 sidecar / fork。
- 不顺手改终端面板其它行为。

## Risks

- 关闭 style-src 的 nonce 注入会让内联样式恢复生效——这是配置**本来就声明**的策略，非新增放宽；`script-src` 不受影响。Tauri 文档标注 **WARNING**，需在 handoff 记明理由。
- 若未来有组件依赖 nonce 才生效的内联样式，行为不变（`'unsafe-inline'` 覆盖更宽）。
