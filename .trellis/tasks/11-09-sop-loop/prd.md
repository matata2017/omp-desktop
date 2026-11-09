# PRD: 把全局 SOP 从阶段门禁改成 LOOP 循环

## Goal

工作单位从「阶段」改为「一次可证伪的实验」。**更深，不更大**：section 数不增，五组件从五个阶段变成同一循环里的五个角色。

产物：`~/.omp/agent/managed-skills/dev-workflow-sop/SKILL.md`（全局，影响所有项目）。

## 已核实的一手事实

| 来源 | 关键机制 |
|---|---|
| Boyd《The Essence of Winning and Losing》(1995/96 最终稿) | 草图标注是 **Decision (Hypothesis)** 与 **Action (Test)**；Orientation 有 Implicit Guidance & Control 直连 Observe/Act，绕过 Decide。通俗四圆圈不是他的模型 |
| Argyris, HBR 1977（第 3/6/10 页，已取到全文） | 温控器喻：单环在既定策略内纠错，双环问「**凭什么是 68 度**」。Exhibit I：Model I → *self-sealing / little testing of theories publicly* → 有效性 Decreased；Model II → *frequent testing of theories publicly* → Increased。**double loop learning always requires an opposition of ideas for comparison** |
| Argyris 同文 Product X 案例 | 坏消息每上跳一层被裁剪、软化、摘要一次，顶层只收到碎片；且**掩盖行为本身不可讨论**，形成 self-sealing 环 |
| Barr et al., *The Oracle Problem in Software Testing* (TSE 2015) | Def 2.7 sound `D(a)⟹G(a)`；Def 2.8 complete `G(a)⟹D(a)`。二者一般不可兼得。裁判分 specified / derived / implicit / human 四类 |
| Rother, Improvement Kata Reference Guide | **Threshold of Knowledge**：没有事实数据的那一点，下一个实验就做在那里；Experimenting Record 记预期/实际/学到 |

本会话的同构证据（我们自己的原始素材）：连改三次判据都没问「该字段是否真的传到 renderer」（self-sealing）；`EXE_CHANGED=False` 照单全收（预消化素材）；`omp=9` 机器级计数误判（无期望值的检查冒充裁判）；先红后绿的回归断言、SHA256、`merge-base --is-ancestor`（真正的裁判）。

## Requirements

- R1 主干改为单循环：`知识阈值 → 可证伪假设 → 最便宜的证伪实验 → 裁判判定 → 记录 → 下一个阈值`。
- R2 明确**内环 / 外环**：内环改产物直到裁判转绿；外环改 governing 变量（假设 / 规格 / 门禁本身）。外环触发条件必须可判定（同类错误复发、或裁判本身可疑），且**必须引入对立观点或独立裁判**，不能只靠自己再想。
- R3 收**裁判四等**（可信度降序）：Specified（有期望值的断言/契约）、Derived（拿另一 artefact 当参照：哈希/字节差分、版本差分、变形关系）、Implicit（无需规格的显然故障：crash/死锁/泄漏/孤儿）、Human（看截图，最弱）。
- R4 收 **soundness 优先：宁可误报，不可假通过**。（`D(a)⟹G(a)` 保证不漏报，代价是误报；不是「宁可漏报」）
- R5 收**原始素材优先**：拒绝经过摘要/裁剪的证据，回到未压缩层（原始日志行、原始帧、字节、进程完整命令行、git 祖先关系）。
- R6 五组件改为循环角色：grilling=提交假设前制造对立观点；ponytail=选最便宜的证伪实验与最小干预；impeccable=UI 表面的裁判；doc-drift=外环（查宣称 vs 实际在用）；Trellis=Experimenting Record。
- R7 **section 数不增**：`grep -c '^## '` ≤ **9**（实测基线）；`wc -l` ≤ **86**（实测基线）。两个都是硬阈值，由脚本从改动前的原文件算出并落盘，改后比对落盘值——基线来自原始文件，不来自记忆。

## 必须存活的既有契约（本次改动自己的裁判）

改后逐条 grep，全命中才算过。不是「我读过觉得没丢」。

| # | 契约 | 锚点（正则） | 强度 |
|---|---|---|---|
| 1 | 一次性运维不进主干 | `一次性运维` | 删除 |
| 2 | 有 `.trellis/` 以其为状态机 | `不臆造` | 极性 |
| 3 | 批准前不碰产品代码 | `批准前不碰产品代码` | 极性 |
| 4 | ponytail 每次编码响应生效 | `每次编码响应` | 删除 |
| 5 | 留一个可运行 check | `可运行 check` | 删除 |
| 6 | impeccable 纯后端不加载 | `不加载` | 极性 |
| 7 | 断言隔离前置条件 | `隔离前置条件` | 删除 |
| 8 | 源码与单测通过不等于交付 | `不等于交付` | 极性 |
| 9 | 模型调用先问 | `模型调用一律先问` | 极性 |
| 10 | 逐条核对不派子代理 | `不派子代理` | 极性 |
| 11 | 不用 ignore 压下去 | `不用.*ignore.*压下去` | 极性 |
| 12 | 代码可疑记 code-todo 且不改代码 | `不改代码` | 极性 |
| 13 | 不替用户提交 | `不替用户提交` | 极性 |
| 14 | 安装器默认已含 magpie | `默认目标已含` | 极性 |
| 15 | 部署后核对哈希 | `哈希一致` | 删除 |
| 16 | 不能只改文本 | `不能只改文本` | 极性 |
| 17 | 详细规则留各自 skill | `改规则改那一处` | 删除 |
| 18 | 用户指令优先 | `用户指令优先` | 极性 |
| 19 | 安全/校验/防数据丢失/可访问性不可简化 | `不可简化` | 极性 |
| 20 | UI 品质不缩水 | `UI 品质不缩水` | 极性 |
| 21 | 偏离记 handoff 分叉清单 | `分叉清单` | 删除 |
| 22 | doc-drift 源码仓路径 | `D:/work/huanghao/doc-drift` | 删除 |
| 23 | 工具入口脚本 | `doc-drift.mjs` | 删除 |
| 24 | 调用式带 `--root` | `--root` | 删除 |
| 25 | 命令由 agent 跑，用户不敲 | `不敲命令` | 极性 |
| 26 | 钩子只做本地记账 | `本地记账` | 删除 |
| 27 | check 只对候选调一次模型 | `调一次模型` | 极性 |
| 28 | doctor 结论写进任务产物 | `结论写进任务产物` | 删除 |
| 29 | audit verify 一条不漏 | `保证一条不漏` | 极性 |
| 30 | 文档落后直接按代码改 | `直接按代码改` | 极性 |
| 31 | 范围外只问不动手改 | `不动手改` | 极性 |
| 32 | grilling vs ponytail 分工 | 人工确认语义 | **允许改写** |

**强度列的含义**：「极性」锚点带否定或限定，留词翻义会被抓；「删除」锚点只防整条消失，**不防翻义**，必须在 A2 里人工过一眼，不能隐含在「全命中」里。第 32 条是唯一允许变字的：原文用 Phase 编号，改 LOOP 后换成循环用语（假设未定 vs 实验实现），语义必须保留。

补锚点的由来：初版表从 `:34` 直接跳到 `:48`，把 `:42/:45/:52-54/:57/:59` 的 doc-drift 操作契约整段漏了。而本次重写正要把它压成「外环」角色并让文件更小，最可能被压缩掉的恰是调用式与接入时机——届时旧表仍报全绿。

## Acceptance Criteria

- A1 写一个可运行检查脚本，对 SOP 逐条 grep 上表锚点，输出命中/缺失并以非零码退出。**先对故意删条的副本跑一次证明它变红**，再对正式版跑绿。
- A2 正式版 **47** 条全命中（**46** 个脚本锚点 + 第 32 条人工）；其中标「删除」的 **22** 条**另附人工语义确认**，且该确认必须**以原文行为单位遍历**，不以锚点为单位——锚点集的盲区就是锚点式复核的盲区。人工那步必须显式记录，不得隐含在「全命中」里。
- A3 硬阈值：`SECTIONS ≤ 9` 且 `LINES ≤ 86`，取自改动前脚本落盘的基线文件，不用记忆值、不留「不显著」这类需人解释的逃生口。
- A4 新 SOP 含 R1–R5 五个机制，且每个都有可判定的触发/退出条件，不是口号。
- A5 `manage_skill update` 后重新读回安装位置，确认落盘内容与预期一致（哈希或逐条 grep）。

## Out of Scope

- 不改 Trellis `workflow.md`、不改四个组件各自的 skill 正文。
- 不动 doc-drift 源码（上一轮已提交 `43e6b3b`）。
- 不碰 advisor TUI task（仍空 planning）。
- 不新增 phase、不新增组件。

## Risks

- `manage_skill update` 是全量覆盖 + 目标是「更小」，正是丢契约的组合 → 由 A1/A2 的 grep 裁判兜住。
- 循环用语取代 Phase 编号后，与 Trellis `workflow.md` 的 Phase 1/2/3 并存可能混淆 → SOP 里明确「项目有 `.trellis/` 时其 Phase 编号仍是状态机，本循环描述的是每一步内部怎么走」。
- 把 SOP 写得更抽象反而不可执行 → 每个机制必须带可判定条件（R1–R5、A4）。
