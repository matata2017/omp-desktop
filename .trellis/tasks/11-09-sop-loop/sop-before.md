---
name: dev-workflow-sop
description: "Global dev-workflow SOP to load at the start of ANY coding task (feature, bug fix, refactor, dependency choice, UI work), or when the user asks for the dev SOP / 开发工作流: one ordered spine of Trellis task lifecycle + grilling plan stress-test + ponytail minimal implementation + impeccable UI polish + doc-drift keeping docs and code in sync, with phase gates, conflict arbitration, and a dogfooding loop for improving doc-drift itself."
---

# 全局开发工作流 SOP

五个组件按一条主干串联，顺序固定：**Trellis 管生命周期，grilling 管计划压力测试，ponytail 管实现体积，impeccable 管 UI 品质，doc-drift 管文档与代码长期对得上**。每步以方括号内可观测条件为准，条件未达成就停在该步。

## 适用范围

任何编码任务：新功能、修 bug、重构、依赖选择、UI 设计与打磨。
纯问答 / 调研 / 翻译 / 总结 / 一次性运维（pull、install、build）不进本主干。

## Phase 0 · 分类 [Gate: 已声明是否建 task]

- 复杂任务（多文件、跨模块、需求模糊、多种方案）→ 取得建 task 同意，进 Phase 1。
- 简单 / 小改动 → 问一句是否建 Trellis task；用户说 no 就本会话跳过 Trellis，仍走 Phase 2–3 的质量门。
- 项目有 `.trellis/` 时以其 `workflow.md` 为状态机；没有时按本 SOP 的 Phase 顺序走，不臆造任务系统。

## Phase 1 · 计划 [Gate: 用户明确批准最终 planning summary]

1. `trellis-brainstorm`：先查仓库事实，再写 `prd.md`；复杂任务补 `design.md`、`implement.md`。
2. `grilling`（用户说 "grill me" / 方案未定 / 决策有分叉时）：按 design tree 逐轮问完整个 frontier，每问给推荐答案；事实自己查（派 sub-agent），决策留给用户；frontier 空且用户确认 shared understanding 才收束。
3. PRD 收敛一遍：合并重复事实、消解临时章节、保留全部 file:line 锚点与验收映射。
4. 呈现最终摘要并**停下**；用户明确批准后才 `task.py start`。批准前不碰产品代码。

## Phase 2 · 执行 [Gate: 验证门全绿 + 改动路径在真实表面跑通]

- `ponytail`（默认 full，**每次编码响应都生效**）：读完整个调用链再爬阶梯 —— 是否根本不需要 / 复用现有 / 标准库 / 平台原生 / 已装依赖 / 一行 / 最小实现。bug 修根因不修症状，留一个可运行 check，删除胜过新增。刻意简化用 `ponytail:` 注释标明天花板。
- `impeccable`（**仅 UI 分支**）：`context.mjs` → 对应 playbook → 编辑前 `craft-floor.md` → 有界验收（一轮批量看全，一轮批量修，最多再确认一轮，然后停）。backend-only 任务不加载。
- 断言要隔离前置条件，不能只看结果：一个守卫有多个 false 分支时，纯结果断言无法归因；同时确认「另一半承诺」（原状态没被破坏）。
- 每步结束跑 `implement.md` 的验证命令，全部改完后统一跑一次全量门。
- 交付面是用户实际运行的产物：改了打包产物就要重建 + 安装 + 在装好的版本上复现改动路径；源码与单测通过不等于交付。

## Phase 3 · 收尾 [Gate: 工作区干净、spec 与文档无漂移]

`trellis-check` → `trellis-update-spec`（把新契约写回 spec）→ **doc-drift 收口**（见下）→ 提交 → `/trellis:finish-work`。重复出现的 bug 类走 `trellis-break-loop`。

## doc-drift：文档 ↔ 代码

自研工具，源码 `D:/work/huanghao/doc-drift`（GitHub `matata2017/doc-drift`），已装技能提供 `scripts/bin/doc-drift.mjs`。命令一律由 agent 跑，用户不敲命令：

```
node "<技能目录>/scripts/bin/doc-drift.mjs" <命令> --root "<项目根>"
```

接入点（**模型调用一律先问，不自动跑**）：

| 时机 | 动作 |
|---|---|
| Phase 1 写完 SPEC | 可选 `doctor`（只查当前任务 SPEC）；用户要求才跑 |
| Phase 2 每轮结束 | 钩子只做本地记账。看到 `[文档漂移 · doc-drift] …` 先做完用户当前请求，再用一道选择题问要不要核对；同意才 `check`（只对那批候选调一次模型） |
| Phase 3 提交前 | **问**要不要跑 `doctor`；同意才跑，结论写进任务产物。要覆盖所有活跃任务用 `doctor full` |
| 用户要求全面体检 | `audit plan` → 逐条核对（**主模型自己做，不派子代理**）→ `audit verify` 保证一条不漏 → `audit review` → 处理 |

处理口径：文档落后于代码 → 直接按代码改；代码有、文档没写 → 直接补；**意图冲突**（文档写"必须/禁止"而代码违背，或代码像 bug）→ 问用户；用户认为代码有问题 → `code-todo` 记下，**不改代码**。已删除的引用去掉代码格式或标「（已删除）」，**不用 `ignore` 压下去**。

范围就是用户说的范围；范围外发现只列出问，不动手改。不替用户提交。

## 工具自举：一边开发一边优化 doc-drift

doc-drift 自己也吃这套 SOP。开发中发现它误报、漏报、文档与实现不符、或安装链路缺口时：

1. 在 `D:/work/huanghao/doc-drift` 改源码（`SKILL.md` 是唯一来源，`bin/` + `lib/` 是实现，`docs/DEV-SOP.md` 是流程文档）。
2. 跑 `npm test`（`test/selftest.mjs`）。
3. `node scripts/install-skill.mjs` 部署，默认目标已含当前 agent 加载的 `~/.config/magpie/library/skills/doc-drift`（连同 `.codex/.claude/.dsh/.workbuddy`，只装已存在的宿主目录）；要装到别处才显式传目录。安装输出会逐个列出目标，核对其中确有 magpie 那条。
4. 部署后核对安装副本与源码哈希一致，再回到原任务。

对它的改动同样要过 Phase 2 的交付门：改完要在真实调用里看到行为变化，不能只改文本。

## 冲突仲裁（单一仲裁处，各 skill 不再各自解释）

| 冲突 | 裁决 |
|---|---|
| 用户当前指令 vs 任一 skill | 用户指令优先 |
| 安全 / 信任边界校验 / 防数据丢失的错误处理 / 可访问性基础 / 用户明确要求 vs ponytail 的更懒 | 前者优先；ponytail 自己列了不可简化项 |
| grilling「未确认不动手」vs ponytail「能默认就别停」 | 分阶段：Phase 1 归 grilling，Phase 2 归 ponytail |
| impeccable「go all out」vs ponytail「laziest」 | 分维度：UI 品质不缩水，实现体积与依赖仍按最小 |
| doc-drift 要调模型 vs 少打扰用户 | 先问再跑；钩子本地判断不调模型，`check`/`doctor` 需用户同意 |
| doc-drift 说改文档 vs 代码看起来才是错的 | 记 `code-todo` 交用户，不擅自改代码 |
| 仓库既有约定 / spec vs 通用习惯 | 前者优先；偏离要记进 `docs/handoff.md` 分叉清单 |

## 不重复

各组件的详细规则留在各自 skill 正文（ponytail 阶梯、impeccable commands 表、grilling round 格式、doc-drift 命令表与判断口径、Trellis `workflow.md`）。本文件只写顺序、门禁与仲裁；改规则改那一处。
