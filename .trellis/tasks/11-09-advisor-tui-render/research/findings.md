# 调研：advisor TUI 渲染错乱

任务：`.trellis/tasks/11-09-advisor-tui-render`。按 SOP 的循环推进，本文件是 Experimenting Record。

## 已确立的事实（两次实验，均在知识阈值上做）

### 实验 1：原始 `terminal_output.data` 是不是完整 ANSI？

独立 spawn `resources/omp.exe --mode rpc-ui --no-auto-resume`，隔离 `PI_CODING_AGENT_DIR`/`OMP_DATA_DIR` 到临时目录并复制模型配置（不复制会因 `Could not restore model` 得到无效样本），发 `tui_open {surface:"advisor_config"}`。**不碰用户正在跑的应用。**

| 观测 | 值 |
|---|---|
| `tui_open` 响应 | `success: true` |
| `terminal_opened` | `cols=100, rows=30, kind=command, title="Advisor configuration", reveal=true` |
| 输出帧数 | 2（`seq=55 len=55`、`seq=5061 len=5006`） |
| 原始流长度 | 5061 |
| ESC / CSI 数 | **289 / 289** |
| ASCII `\|` 数 | **0** |

原始头含 `\u001b[?1049h`（备用屏）、`\u001b[1;1H`、`\u001b[J`、`\u001b[38;5;236m`，边框是 **`│` U+2502 盒线字符**而非 ASCII `|`。

`seq` 与 JS 字符串长度一致（55+5006=5061=末帧 seq），**此样本未见 byte/char 计数错位**。

**结论**：sidecar 发出的是完整正确的 ANSI TUI，宽度 100。缺陷不在 PTY/sidecar 侧。

### 实验 2：TUI 认不认 `terminal_resize`？

同一隔离方式，`tui_open` 后发 `terminal_resize {cols:40, rows:20}`。

| 观测 | 值 |
|---|---|
| 重绘前最大可见宽度（剥 ANSI 后） | **101** |
| `terminal_resize` 响应 | `success: true` |
| 重绘后输出长度 | 2053 |
| 重绘后最大可见宽度 | **41** |
| 重绘后 CSI 数 | 176 |
| 判定 | **HONORS_RESIZE** |

**结论**：sidecar 收到 resize 会按新宽度正确重绘。**缺陷 100% 在 renderer 侧**：要么没发 `terminal_resize`，要么发的时机/值与 xterm 实际宽度不一致。

## 症状的物理解释（由事实推出，非猜测）

用户截图里「一列竖线」= 100 列宽的表格被更窄的 xterm 逐行折断，盒线 `│` 落在不同折行上堆成竖列。不是数据损坏，是**宽度不匹配**。

## 下一个知识阈值

**renderer 实际发了什么 `terminal_resize`、xterm 当时的真实 cols 是多少？**

已知代码路径（静态，尚未运行时验证）：
- `terminals.ts:133-163` `createView` 用 `info.cols/info.rows`（=100/30）建 xterm，注册 `term.onResize` → 发 `terminal_resize`
- `TerminalPanel.tsx:164-190` `useLayoutEffect` → `term.open(host)` → `fitToContainer()`；**:175 容器宽或高为 0 时直接 return**，随后 `ResizeObserver` 兜底
- `terminals.ts:200-227` `terminal_opened` 先 `createView` 再 `upsertInfo(reveal)`；输出帧在 xterm 尚未 `open()` 时就被写入

待验证假设（**未确认，不得据此改代码**）：
- H1：首帧 100 宽内容在 xterm 仍为 100 列时写入并被折行；随后 fit 改宽 + resize 重绘，但折行残留留在缓冲里
- H2：`fitToContainer` 因容器 0 尺寸跳过，xterm 停在 100 列而容器更窄，`terminal_resize` 从未发出
- H3：resize 发了但值与 xterm 实际渲染宽度不一致

区分它们需要**运行时观测**，不是再读代码。

## 阻塞点

当前 `ROMP=1 / CDP=0`——用户在跑自己起的应用，没开调试端口。观测真实 renderer 需带 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` 重启，会打断用户会话。**需用户决定。**

## 复现资产

- `.probe/advisor-tui-probe.mjs`、`.probe/advisor-resize-probe.mjs`（隔离 spawn，可重跑）
- `.probe/advisor-raw.txt`（5061 字符真实 ANSI 流）、`.probe/advisor-frames.json`、`.probe/advisor-after-resize.txt`

这些是回放素材：可在不碰 GUI 的前提下把真实流喂给渲染层做回归。

---

## 实验 3–6：定位到呈现层（用户关闭应用后继续，未打断）

### 实验 3：真实 GUI 里 renderer 发了什么

带 CDP 启动安装版，hook `window.omp.rpc.command/commandForTab` 记录 `terminal_*`，点 advisor 眼睛（按 lucide `svg.lucide-eye` 定位——`aria-label` 是中文本地化文本，按 `/advisor/i` 匹配不到）。

- `terminal_resize` **确实发出**，`cols=72 rows=48`（两次）
- `terminal_list` 回读 `cols=72` —— sidecar 接受
- viewport 537px / xterm 529px

**H2「resize 从未发出」被证伪。**

### 实验 4：sidecar 在 72×48 直出，与 GUI 对比

独立 spawn 后 resize 到 72×48，用极简 ANSI 屏幕模型还原网格：

```
╭─ Advisor configuration ·…┬──────────────────────────────────────╮
│ ❯ ● default              │ default                              │
│   Shared instructions    │ Enabled: ● on                        │
╰──────────────────────────┴──────────────────────────────────────╯
```
`MAX_W=72 NONEMPTY=40 BOX_ROWS=37` —— **布局完全正确**。

### 实验 5：xterm DOM 的实际内容

枚举全部 `.xterm`（只有 1 个）：`maxLen=72 nonEmpty=40 boxRows=37`，首行 `╭─ Advisor configuration ·…┬───…`。

**与 sidecar 直出网格逐项吻合。数据链路 100% 正确。**

（一次早期测量得到全空，是取样过早；差点据此误判。教训同 SOP「隔离前置条件」。）

### 实验 6：权威截图 + 计算样式 —— 根因所在

`capture-window.ps1` 抓到的是被遮挡的其他窗口（Human 档裁判失效），改用 **CDP `Page.captureScreenshot`** 取 WebView 真实内容：症状确实存在且持久，空行显示为 **`│ │ │` 三根竖线**。

而 DOM 里该行是 `│········│········│`（两根 + 大量空格）。计算样式：

| 属性 | 实际 | 应为 |
|---|---|---|
| `.xterm-rows` `white-space` | **`normal`** | `pre` |
| `font-family` | **应用 UI 比例字体**（Inter Variable / PingFang SC…） | monospace |
| `font-size` | **15px** | xterm 配置的 12px |

**空格被折叠** → 右边框左移 → 空行的两根边框加相邻行边框在视觉上堆成三根竖线。这不是折行，是**空白折叠 + 比例字体**。

## 已确认的根因类别

**数据全对，缺陷纯在呈现层**：xterm 的 DOM renderer 未能把 `white-space: pre` 与 monospace 字体应用到 `.xterm-rows`。

补充事实：xterm.css **已加载**（35 条 `.xterm` 规则），但其 `white-space: pre` 挂在 **`.xterm-accessibility-tree`**，vendor CSS 里**没有 `.xterm-rows` 规则**——rows 的字体/空白由 DOM renderer **运行时注入的 per-instance 样式表**（`.xterm-dom-renderer-owner-N`）负责。应用 `components.css:1712-1718` 只设 height/scrollbar，不是覆盖源。

## 下一个知识阈值（未验证，不得据此改代码）

**DOM renderer 注入的那张 per-instance 样式表到底存不存在、内容是什么、被谁覆盖？**

观测点：`document.styleSheets` 里带 `.xterm-dom-renderer-owner-1` 前缀的规则；`.xterm` 根元素的 inline style；`--font-mono` 变量的实际值（`createView` 用它作 fontFamily，若它解析成 UI 字体则字体问题另有来源）。

需要应用重新带 CDP 启动才能取到。当前 `ROMP=0`。

## 裁判设计（初版，已被下方「裁判设计修正」取代）

初版把 CSS 属性当通过条件，**不 sound**：硬加 `white-space:pre; font-family:monospace` 即可假绿。保留此节仅为记录演进，实际判据以下方修正节为准。

回放资产 `.probe/advisor-raw.txt` / `advisor-72-grid.txt` 供离线回归。

---

## 离线定案（不依赖应用，来自本地 vendor sourcemap）

`node_modules/@xterm/xterm/lib/xterm.js.map` 带 `sourcesContent`，直接读出 `src/browser/renderer/dom/DomRenderer.ts`：

```ts
// _injectCss :156-159
if (!this._themeStyleElement) {
  this._themeStyleElement = this._document.createElement('style');
  this._screenElement.appendChild(this._themeStyleElement);   // 挂在 .xterm-screen 内部，不是 head
}
// :163-169
`${this._terminalSelector} .${ROW_CONTAINER_CLASS} {`
  + ` color: …; font-family: ${rawOptions.fontFamily};`
  + ` font-size: ${rawOptions.fontSize}px; font-kerning: none; white-space: pre }`
// :275
this._themeStyleElement.textContent = styles;
```

`_terminalSelector` = `.xterm-dom-renderer-owner-N`（与实测 `xtClasses` 一致），`ROW_CONTAINER_CLASS` = `xterm-rows`。注入时机是 `activate()` :80。

### 据此划掉的分支

| 分支 | 判定 | 依据 |
|---|---|---|
| `--font-mono` 被定义成 UI 字体 | **排除** | `global.css:16` 静态就是 `ui-monospace, "SF Mono", …, monospace`；`:15` 的 `--font-body` 才是实测那串 sans。rows 在**继承 body 字体**，配置源头没问题 |
| xterm.css 没加载 | **排除** | `sheetHrefs` 有 `xterm-LZoznX6r.css`，命中 35 条 |
| app CSS 覆盖 xterm.css | **排除** | vendor CSS 全文**无 font-family 规则**，`white-space:pre` 只在 `.xterm-accessibility-tree`；`components.css:1712-1718` 只设 height/scrollbar |
| 应用崩溃 | **排除** | `runtime.log` 尾部是正常 idle，无 panic/异常退出 |

### 收敛后的根因假设（待一次定向验证）

**DOM renderer 注入的 `_themeStyleElement` 缺失或为空**，因此 `.xterm-rows` 拿不到 `white-space:pre` 与 monospace，退回继承 body 的 `normal` + 15px sans。

支持证据：`styleTags:3` 但 `sheetCount:4`（3 link + 1 inline），且那张 inline 表 `xtermRuleHits=0`。

**定向探针（重启后一条即可判）**：
```js
document.querySelectorAll('.xterm-screen style')  // 存在几个？
// 每个的 textContent 长度、是否出现在 document.styleSheets、cssText 前 200 字
```
存在且非空 ⇒ 是被移除/未解析；不存在 ⇒ 注入从未发生（`activate` 路径问题）。

## 裁判设计修正（原设计不 sound）

原提的 `whiteSpace==='pre' && fontFamily 含 monospace` **会假绿**：在 app CSS 里硬加这两条即可转绿，而注入表为何失效根本没修——正是「锚标签 vs 锚实质」。

**主裁判改为可观测的实质**：渲染行的**视觉宽度 == DOM 文本宽度**；或第 27 列的 `│` 实际 x ≈ `27 × cellWidth`。CSS 属性只作辅助诊断，不作通过条件。

补充裁判（Derived，离线可跑）：用**真 xterm** 而非手搓 ANSI 模型回放 `.probe/advisor-raw.txt`——`new Terminal({cols,rows})` → `write()` → `resize()` → `buffer.active.getLine(y).translateToString()`，这些是 public API，**不需要 `open()`、不需要 canvas、不必新增依赖**。

## 修复位置的硬约束

`docs/handoff.md:371` 白名单：`src/renderer` 字节级改动仅限 `main.tsx / index.html / ChatStream.tsx / use-session-switch.ts / LangSwitcher / ContextStrategyBar / ContextUsagePopover(+test) / locales`。

**`TerminalPanel.tsx` 与 `styles/components.css` 都不在名单内。** 修复要么放进已白名单的 `index.html`，要么改完**显式登记**进 handoff 白名单 + parity 分叉清单，不得静默改「保持上游」的文件。

另：字体若从 15px sans 变成 12px mono，**字符宽度改变 → `fit()` 算出的 cols 也变**，修完必须重新测，不能沿用 72。
