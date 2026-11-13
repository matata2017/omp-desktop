---
name: eng-dev-sop
description: 我们做开发的标准流程（SOP），四个技能分工协作：grilling 压方案 → Trellis 管任务与规范 → ponytail 写最小实现 → doc-drift 收口文档。触发词：做个功能、加个需求、改这块、修个 bug、开工、按SOP来、走开发流程。适用于代码项目；不适用于投研分析、资料查询、纯问答。
---

# 开发 SOP：grilling → Trellis → ponytail → doc-drift

四个技能各管一段，不重叠、不互相替代。冲突时以仓库里的 `.trellis/workflow.md` 与 `.trellis/spec/` 为准。

| 技能 | 管什么 | 什么时候进 |
|---|---|---|
| `grilling` | 把方案问穿：设计树 + 逐轮问 frontier 问题，每个问题给推荐答案 | 需求/方案有分支没定，动手前 |
| `trellis` | 任务台账与分层规范：任务目录、`prd.md`、`.trellis/spec/`、check、归档 | 全程（主线骨架） |
| `ponytail` | 最小完整改动：YAGNI、stdlib 优先、短 diff、该补的调用方/测试不落下 | 写代码时 |
| `doc-drift` | 文档与代码逐条对账，以代码为准修文档 | 收尾，代码稳定后 |

## 网络（本机实测，2026-11）

- **GitHub 单文件**（raw）：首选 `https://ghfast.top/https://raw.githubusercontent.com/...`（0.8 s）；次选 jsDelivr `https://cdn.jsdelivr.net/gh/<owner>/<repo>@<ref>/<path>`（1.6 s）；走代理直连 raw 约 2.6 s，可兜底。`gh-proxy.com` 前缀对 raw 最慢（3.8 s），只在 ghfast 失效时用。裸 `raw.githubusercontent.com` 直连不通。
- **GitHub 仓库**：`git clone` 直连会被 reset；加代理 `-c http.proxy=http://127.0.0.1:7897` 最快。镜像前缀 `https://gh-proxy.com/https://github.com/...` 也能 clone 成功（已验完整性），但慢（90 s+），只在大文件/代理不可用时用。
- **cargo**：`~/.cargo/config.toml` 已指向 `sparse+https://rsproxy.cn/index/`（crates.io 直连 schannel 失败）。
- **npm/bun**：`registry https://registry.npmmirror.com`（`D:/rust-omp/bunfig.toml` + `npm config`）。
- **PyPI**：`-i https://pypi.tuna.tsinghua.edu.cn/simple`。

## 前提（缺哪个先补）

- 项目根有 `.trellis/`：没有就跑 `trellis init -y -u <开发者名> --omp`（生成 `.trellis/` + `.omp/skills/trellis-*`）。
- doc-drift 工具在本机 `D:/doc-drift`：项目首次跑 `node D:/doc-drift/bin/doc-drift.mjs init --no-model --root <项目根>` 生成 `.doc-drift/`。**没 init 就跑其它子命令会抛 `Cannot read properties of null (reading 'docInfo')`**（工具缺 null-config 守卫），见到这个报错就是还没 init，别当成文档问题。
- 初始化后优先用**项目内** `.omp/skills/trellis-*`（带完整流程与 references）；全局 `trellis` 技能只是路由入口。

## 步骤（每步可检查）

1. **定位**：读 `.trellis/workflow.md` 当前阶段 + `git status` + `.trellis/tasks/` 活跃任务；没有对应任务就先建任务目录。
2. **压方案**（`grilling`）：把未定的分支列成设计树，一轮问完当前 frontier，每个问题带推荐答案，等用户答完再下一轮；事实自己查，不甩给用户。frontier 空了才算方案定。
3. **落需求**（`trellis-brainstorm`）：把定下来的方案写成任务目录的 `prd.md`（范围 + 验收标准），一次只问一个真正缺的信息。
4. **读规范**（`trellis-before-dev`）：按包/层读 `.trellis/spec/` 的 pre-development checklist。
5. **实现**（`ponytail`）：按 `implement.jsonl` 给的上下文写代码；先找现成的 helper/依赖，能一行就一行；改动必须覆盖所有调用方、测试、fixture；不自动 git commit。
6. **验证**（`trellis-check`）：对照 spec 跑 lint / type-check / test，查跨层数据流；能修的自己修。
7. **文档收口**（`doc-drift`）：`doc-drift doctor`（默认只查当前 SPEC），必要时 `audit plan` → 逐条核对 → `audit verify`。文档落后按代码改；意图冲突用选择题问用户；代码问题记"代码待改"，不改代码。结论与证据写进任务 `MILESTONES.md`。
8. **收尾**（命令 `trellis-finish-work`，见 `.omp/commands/`）：过质量门、提示提交、归档任务、`trellis-update-spec` 回写经验与 journal。

## 交付

- 任务目录 `prd.md`、工作区代码改动（不自动提交）、`.doc-drift/reports/` 体检报告、归档任务 + journal。
- 提交由用户定（分支 / 时机）。

## 完成标准（逐条核）

- [ ] 方案分支已问穿（frontier 空），或明确记录哪些没定及原因
- [ ] 有 Trellis 任务与 `prd.md`，范围与验收标准写清
- [ ] 写码前读过对应层 `.trellis/spec/`
- [ ] `trellis-check` 通过（lint / type-check / test）
- [ ] `doc-drift doctor` 跑过，文档与代码对齐（或冲突已问、代码待改已记）
- [ ] `trellis-finish-work` 归档，journal / spec 已更新

## 禁止

- 不跳过 spec 凭感觉写；不在流程里自动 commit / push；不改用户手写文档而不说明；不擅自把 doc-drift 的"代码待改"当任务去改代码；不用 ponytail 的"最小"当借口砍掉验收标准要求的范围。
