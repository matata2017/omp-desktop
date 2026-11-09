# 验证证据：advisor TUI 修复（CSP nonce）

## 根因

Tauri 编译时往 `style-src` 注入 nonce → CSP 规范下 `'unsafe-inline'` 被忽略 → xterm `DomRenderer` 在 `.xterm-screen` 内创建的**无 nonce** `<style>` 被拦（`sheet=null`）→ `.xterm-rows` 继承 body 的 `white-space:normal` / sans / 15px → 空格折叠 → 边框漂移成竖列。

## 改动

`src-tauri/tauri.conf.json` 的 `app.security` 增加一行：

```json
"dangerousDisableAssetCspModification": ["style-src"]
```

键与列表语义在 `tauri-utils-2.10.2/src/config.rs:3071-3083` 与 `@tauri-apps/cli/config.schema.json` 双处核实。`script-src` 未动，`src/renderer` 零改动（绕开 `handoff:371` 字节级白名单）。

## A5 落地 + 生效（主判据）

安装版 `fetch(location.href)` 读有效响应头：

| | 修复前 | 修复后 |
|---|---|---|
| `style-src` | `'self' 'unsafe-inline' 'nonce-2043626238886117981'` | **`'self' 'unsafe-inline'`** |
| 含 nonce | 是 | **否** |
| `script-src` sha256 数 | — | **36**（保护完好） |

本次只改配置、renderer 未动，`dist/assets/index-BxxRkkvB.js` **与前两次构建同名**，故「查内嵌资源名变化」的旧验证法失效；响应头判据同时证明新二进制落地与修复生效。

## A1 → A2 宽度裁判红绿闭环

同一裁判、同一独立参照（`cellWidth = rowsWidth / ptyCols`，**现场重取**，非由被测点反推）、同一行同一列：

| 量 | 修复前 | 修复后 |
|---|---|---|
| 独立参照 | `518 / 72 = 7.194` | `518 / 72 = 7.194`（重取，恰好相同） |
| 盒线列 | `[0, 27, 71]` | 同 |
| 实测 x | `[0, 94.89, 163.5]` | **`[0, 194.61, 511.72]`** |
| 期望 x | `[0, 194.25, 510.81]` | 同 |
| **残差** | **`[0, −99.36, −347.31]`** | **`[0, 0.36, 0.91]`** |
| `WIDTH_MATCHES` | **false** | **true** |

A3 机制辅助：两张注入表 `hasSheet` `false/false` → **`true/true`**；`white-space` `normal` → **`pre`**；字体 sans → **`ui-monospace`**；字号 15px → **12px**。

A4：`ptyCols` 与 `rowsWidth` 均现场重取，未沿用旧值。

## A7 更宽的修复面

`style-src` 是文档级的。出厂 bundle 静态核实：`codemirror-DH2DLpYI.js` 有 `createElement("style")`，`mermaid-MrnEQlGY.js` 有两处且无 `adoptedStyleSheets` 分支——它们与 xterm 撞同一条 CSP。

**能力探针（决定性）**：在安装版注入一个**无 nonce** 的 `<style>`：

```
hasSheet: true, ruleCount: 1, appliedColor: "rgb(1, 2, 3)"
CAPABILITY_RESTORED: true
```

文档级佐证：`style` 标签 **3/3 有 sheet、0 被拦**（修复前 1 有 2 拦）。

**CodeMirror 面已实测通过**（`ComposerEditorDialog`，Ctrl+G 打开、双 Esc 丢弃，无持久副作用）：

```
cmEditor: true, cmContent: true
cmStyleSheets: [{ len: 6547, hasSheet: true }]
blocked: 0, cmAfterEsc: false
A7_CODEMIRROR: true
```

**归因更正**：初版把未测到记成「导航未进入编辑器」，**这是错归因**。真因是**指认错组件**——源码中引用 CodeMirror 的只有 `ComposerEditorDialog.tsx:9-11` 与 `ExtensionDialog.tsx:14-15`，`TaskEditorDialog` 根本不含它，所以走定时任务→新建任务那条路再久也量不到 `.cm-editor`。PRD 的 A7 例子已同步改正，避免下个 agent 重复无效导航。

**断言自身的 bug**：首次跑出 `A7_CODEMIRROR:false` 是假阴性——`cmStyles` 存的是原始 DOM 元素，`s.hasSheet` 并不存在（该属性只在 map 出的数组里），`.every()` 恒 false。改用映射后的数组重跑得到 true。方向上属误报而非假通过，但仍是错的裁判。

**仍未执行**：mermaid 面未挂载，不声称已冒烟；其依据仅为上述文档级能力探针与 bundle 静态核实。

## 视觉确认（Human 档，仅作补充）

CDP `Page.captureScreenshot`：左右两栏对齐、边框闭合、`❯ ● default` 选中态、右栏分块清晰、底部操作提示完整。不再出现竖线列。

## 过程中修掉的观测缺陷

1. **裁判自证**：初版用两根盒线反推 `cellWidth`，两点定直线残差恒 0，**可证必然通过**。改为独立参照 + 要求 ≥3 根、跨列 >20。
2. **取样过早**：固定 sleep 取到全空。改为轮询至 `nonEmpty>=5`。
3. **截错窗口**：`capture-window.ps1` 的 PrintWindow 抓到被遮挡的其他窗口。改用 CDP `Page.captureScreenshot`。
4. **target 兜底**：`scripts/cdp-eval.mjs` 与 `.probe/cdp-shot.mjs` 的 `!startsWith("http")` 在安装版（`http://tauri.localhost/`）上不匹配，静默落到「第一个 page」。两处均改为显式匹配 `tauri.localhost`。
