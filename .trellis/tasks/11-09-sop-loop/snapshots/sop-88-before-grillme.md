---
name: dev-workflow-sop
description: "Global dev-workflow SOP to load at the start of ANY coding task (feature, bug fix, refactor, dependency choice, UI work), or when the user asks for the dev SOP / 开发工作流: the unit of work is one falsifiable experiment, not a phase — knowledge threshold → hypothesis → cheapest red-capable test → oracle verdict → record, with an inner loop that fixes the artifact and an outer loop that questions the governing assumption. Ranks oracles (specified/derived/implicit/human), prefers soundness over completeness, demands raw unsummarized evidence, and casts Trellis / grilling / ponytail / ub-mode / impeccable / doc-drift as roles in that one loop."
---

# 全局开发工作流 SOP

工作单位是**一次可证伪的实验**，不是阶段。每轮必须产出：一个假设、一个不是我的裁判、一条证据增量。六个组件是同一循环里的角色，不是六道关卡。

## 适用范围

任何编码任务：新功能、修 bug、重构、依赖选择、UI 设计与打磨。纯问答 / 调研 / 翻译 / 总结 / 一次性运维（pull、install、build）不进本循环。

## 循环

`知识阈值 → 可证伪假设 → 最便宜的证伪实验 → 裁判判定 → 记录 → 下一个阈值`

- **知识阈值**：事实与数据用尽的那一点。下一个实验做在那里，不做在好做的地方。
- **假设**：写成「若 X 成立，则改 Y 会让现象消失 / 改 Z 会更糟」。说不出预测的不是假设，是感觉。
- **实验**：选最便宜且**能变红**的那个；跑之前先确认它在当前状态下确实红。
- **记录**：预期 / 实际 / 学到什么，写进任务产物；失败的实验同样记。
- 项目有 `.trellis/` 时其 Phase 编号仍是状态机，本循环描述每一步内部怎么走；没有时按本循环走，不臆造任务系统。
- 复杂任务先取得建 task 同意；假设未经用户批准前不碰产品代码。
- **符号级问题只走 LSP**：定义 / 引用 / 实现 / 类型 / 诊断 / 跨文件重命名一律 `xd://lsp`，**不用 grep 冒充**。grep 只用于非符号文本（文案、JSON、文档、日志、CSS）与「先定位文件」；文件已定位就换 LSP。改导出符号前先 `references`。没有对应 language server 或报错时**在回复里说明**（语言 / 动作 / 错误 / 替代手段），不许静默降级。

## 内环与外环

- **内环**：改产物，直到裁判转绿。
- **外环**：改 governing 变量——假设 / 规格 / 门禁本身。触发条件（任一可判定）：① 同一类错误第二次出现；② 裁判本身可疑（期望值写错、锚点无极性、计数无期望值）。
- 外环**必须引入对立观点或独立裁判**，不能只靠自己再想一遍。连续两次内修无效即强制进外环。

## 裁判（可信度降序，优先高档）

| 档 | 判据 | 例 |
|---|---|---|
| Specified | 有期望值的断言 / 契约 | 先红后绿的回归断言；teardown 后 sidecar 数 = 0 |
| Derived | 拿另一 artefact 当参照 | 构建产物 vs 安装副本哈希 / 字节差分；版本差分；变形关系 |
| Implicit | 无需规格的显然故障 | crash、死锁、泄漏、孤儿进程 |
| Human | 人看一眼 | 看截图判断（最弱，只作补充） |

- **soundness 优先：宁可误报，不可假通过**。判「通过」的必须真对；代价是可能误报，接受。
- 断言要**隔离前置条件**：守卫有多个 false 分支时纯结果断言无法归因；同时确认另一半承诺（原状态没被破坏）。
- 无期望值的计数不是裁判。锚点要带否定或限定，否则「留词翻义」同样假通过；人工复核按**原文逐行**对照，不按锚点逐条——锚点集的盲区就是锚点式复核的盲区。报 MISS 先怀疑模式写法（转义、`--` 分隔、计数器把不同失败混成一个数），再怀疑内容。
- 留一个**可运行 check**。**交付面**是用户实际运行的产物：改了打包产物就重建 + 安装 + 在装好的版本上复现改动路径；源码与单测通过**不等于交付**。

## 原始素材

拒绝经过一层摘要的证据。回到未压缩层：原始日志行、原始帧、字节、进程完整命令行、git 祖先关系。坏消息每上跳一层就被裁剪软化一次，摘要里的「成功」可能是被裁掉的失败；工具输出被截断时取回原文再下结论。

## 六个角色

- **grilling**：提交假设前制造对立观点，逐轮问完 frontier；事实自己查，决策留给用户。
- **ponytail**（默认 full，**每次编码响应**生效）：选最便宜的证伪实验与最小干预；修根因不修症状，删除胜过新增。
- **ub-mode**（与 ponytail 同档，**每次编码响应**生效）：先 Make it Work（最小可运行；先红后绿），再 Make it Right（根因、Crap/Dryer/Mutator、行为不变），最后 Practice（行动日志、上下文守卫、清理、提交推送）。与 ponytail 冲突时：最小干预归 ponytail，顺序与「先工作再正确」归 ub-mode。
- **impeccable**（仅 UI）：UI 表面的裁判，有界轮次；backend-only 任务**不加载**。
- **doc-drift**：外环执行器，查「宣称 vs 实际在用」。
- **Trellis**：Experimenting Record，抗压缩的持久记忆。

## doc-drift 操作契约

源码 `D:/work/huanghao/doc-drift`（GitHub `matata2017/doc-drift`），入口 `doc-drift.mjs`。命令由 agent 跑，用户**不敲命令**：

```
node "<技能目录>/scripts/bin/doc-drift.mjs" <命令> --root "<项目根>"
```

**模型调用一律先问**：钩子只做**本地记账**；看到 `[文档漂移 · doc-drift]` 先做完当前请求再问；同意才 `check`（只对候选**调一次模型**）。写完 SPEC 可选 `doctor`（**只查当前任务 SPEC**）；提交前问要不要跑，同意才跑、**结论写进任务产物**；要覆盖所有活跃任务用 `doctor full`。
全面体检：`audit plan` → 逐条核对（**主模型自己做，不派子代理**）→ `audit verify` **保证一条不漏** → `audit review`。
口径：文档落后**直接按代码改**；代码有文档没写直接补；**意图冲突**（文档写"必须/禁止"而代码违背，或代码像 bug）问用户；代码可疑记 `code-todo` 且**不改代码**。已删引用去掉代码格式或标「（已删除）」，**不用 `ignore` 压下去**。**范围就是用户说的范围**，范围外只列出问、**不动手改**；**不替用户提交**。

## 工具自举

doc-drift 自己也吃本循环。改源码 → `npm test` → `node scripts/install-skill.mjs`（**默认目标已含**当前 agent 加载的 `~/.config/magpie/library/skills/doc-drift`，核对输出确有该条）→ 核对安装副本与源码**哈希一致**。改完要在真实调用里看到行为变化，**不能只改文本**。 **改既有 agent 文档（本 SOP、各 skill）默认用 edit/追加，全量覆盖仅限新建**——两次实测事故都是覆盖造成段内改字或丢句，而关键词 grep 全绿。前置门：改前落盘快照 + 改后**逐行 diff** 作独立裁判，diff 里每条删除行都要能说明是有意的；grep 只作补充。

## 冲突仲裁

| 冲突 | 裁决 |
|---|---|
| 用户当前指令 vs 任一 skill | **用户指令优先** |
| 安全 / 信任边界校验 / 防数据丢失 / 可访问性 / 用户明确要求 vs 更懒 | 前者优先，**不可简化** |
| grilling「未确认不动手」vs ponytail「能默认就别停」 | 假设未定归 grilling，实验实现归 ponytail |
| ponytail「最便宜」vs ub-mode「先 Work 再 Right」 | 同一次改动取最小实现；先让它可运行，再补正确性与测试，不为「正确」提前加抽象 |
| impeccable「go all out」vs ponytail「laziest」 | **UI 品质不缩水**，实现体积与依赖仍最小 |
| doc-drift 调模型 vs 少打扰 | 先问再跑 |
| doc-drift 说改文档 vs 代码才是错的 | 记 `code-todo` 交用户 |
| 仓库既有约定 vs 通用习惯 | 前者优先；偏离记进 `docs/handoff.md` **分叉清单** |

详细规则留各自 skill 正文；本文件只写循环、裁判与仲裁，**改规则改那一处**。
