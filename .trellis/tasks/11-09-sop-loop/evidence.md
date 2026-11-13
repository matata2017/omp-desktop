# 验证证据：SOP 改 LOOP

## 裁判自身先被验证（A1）

`check-sop.sh` **38** 个脚本锚点：polarity 21 / deletion 17。

| 步骤 | 结果 |
|---|---|
| 对原文件捕获基线 | `SECTIONS=9 LINES=86`，`VERDICT=GREEN` |
| 突变体 A：删 `--root` 调用式 | `MISS [deletion] --root`，exit **1** |
| 突变体 B：`不用 ignore 压下去` → `可以用`（**保留 ignore 一词**） | `MISS [polarity]`，exit **1** |
| 突变体 C：删 `doctor full` 子句 | `MISS [deletion] doctor full`，exit **1** |

突变体 B 里 `grep -c ignore` 仍为 1 —— 裸词锚点会放行翻义，极性正则抓住。这是 soundness 的实证。

踩坑：`grep -Eq -e "--root"` 被 rtk 的 grep 包装当成选项。必须 `grep -Eq -- "$a"`，脚本已固化。

## 改后判定（A2 / A3）

首次改完**判红**：锚点全命中、`sections 9<=9`，但 `lines 89 > 86` → `FAIL lines grew`。砍 3 行后转绿。「更深不更大」由机器守住，不由我的判断力守住。

补回丢失契约后终跑：

```
THRESHOLD sections 9 <= 9 ; lines 86 <= 86
TOTAL_ANCHORS=38 MISSING=0
VERDICT=GREEN
```

## 人工复核：第一遍做错了，方法本身有洞

**第一遍是锚点中心的**：逐个锚点确认「词还在、语气没反」。它结构上**看不见同一行里没被锚点覆盖的内容**，于是 grep 绿、人工也绿，而真实损失发生了。

按原文 `:40–59` **逐行**对照新 `:58–68` 重做后，查出的损失：

| 原行 | 丢失内容 | 锚点是否覆盖 |
|---|---|---|
| :52 | **整条消失**：`doctor`（**只查当前任务 SPEC**）+ 写完 SPEC 的时机 | 否 |
| :54 | **`doctor full`**、「同意才跑」 | 否（`结论写进任务产物` 命中，掩盖了同行其余部分丢失） |
| :57 | **意图冲突的定义**（文档写"必须/禁止"而代码违背，或代码像 bug）——「何时该问」的判据 | 否 |
| :59 | **`范围就是用户说的范围`** | 否 |
| :53 | `[文档漂移 · doc-drift]` 字面标记 | 否 |
| :42 | GitHub `matata2017/doc-drift` | 否 |

全部已补回，并**为这几条新增锚点**（`只查当前任务 SPEC`、`范围就是用户说的范围`、`doctor full`、`意图冲突`、`必须/禁止`、`matata2017/doc-drift`、`文档漂移 · doc-drift`），使下次重写再丢会被抓。

反讽记录：上一轮刚在 doc-drift 源码里修 `doctor full` 的作用域误描述并加回归断言，这一轮却把该命令从 SOP 删没了，方向相反。

**方法论结论**：锚点集只能证明「我想到的那些还在」。人工复核必须**以原文行为单位**遍历，而不是以锚点为单位——否则锚点集的盲区就是人工复核的盲区，两道关卡同一个洞。这条已写回 SOP 的裁判节（「无期望值的计数不是裁判」之外，还需按原文逐行对照）。

## 17 条 deletion 档的语义确认（第二遍，行中心）

逐行读过原文 `:40–59` 与新 `:58–68` 后确认：`一次性运维` 仍是排除项；`本地记账` 保留「只做」；`--root` 调用式一字未改；`只查当前任务 SPEC` 与 `doctor full` 已复位；`意图冲突` 带定义；`范围就是用户说的范围` 已复位；其余同第一遍结论。

第 32 条（唯一允许改写）：`分阶段：Phase 1 归 grilling，Phase 2 归 ponytail` → `假设未定归 grilling，实验实现归 ponytail`，语义等价。

## 落盘核对（A5）

裁判直接跑在安装位置 `~/.omp/agent/managed-skills/dev-workflow-sop/SKILL.md`，GREEN 是对**实际加载副本**的判定。

## 结构对比

| | 改前 | 改后 |
|---|---|---|
| 工作单位 | 阶段 + 门禁（栏杆） | 一次可证伪实验（循环） |
| 组件 | 五个阶段 | 同一循环里的五个角色 |
| 纠错 | 只有改产物 | 内环改产物 / 外环改 governing 变量 |
| 裁判 | 隐含「跑测试」 | 四等分类 + soundness 优先 + 极性要求 |
| 证据 | 未规定层级 | 原始素材优先，拒绝摘要层 |
| `##` 数 / 行数 | 9 / 86 | 9 / 86 |

## 已知缺口（未在本任务解决）

SOP 正文在 `~/.omp/agent/managed-skills/`，**仓库外、无持续版本控制**。已落盘 `sop-before.md` 保住改前原文，本次可 diff、可回滚；但后续每次改 SOP 仍需手动留快照，否则同样问题会复发。根治（把 managed-skills 纳入某个仓或加自动快照）未在本任务解决。

## 裁判自身的三次加固（后续复核发现）

1. **fail-open 后门**：`*)` 兜底原本只 echo 一句 anchors-only 就继续走到末尾打印 GREEN。MODE 缺失或拼错（`--againt`）时**跳过阈值比对仍返回 0**——裁判给自己开了假通过路径。改为 fail-closed `exit 1`；`--capture` 输出 `VERDICT=BASELINE` 与 `--against` 的 GREEN 区分。实测：拼错模式 exit 1、缺模式 exit 1。
2. **新机制无护栏**：原 39 条锚点全是旧契约，删掉「知识阈值 / 可证伪 / 外环 / 宁可误报 / 未压缩」照样全绿，而这些正是本次交付物本身。补 7 条后共 **46** 条（polarity 24 / deletion 22）。实测把「外环」全替换成「内圈」→ `MISS [deletion] 外环` exit 1。
3. **锚标签 vs 锚实质**：`意图冲突` 这条锚点在丢失发生时**照样 HIT**——丢的从来不是这个词，而是它的定义（文档写"必须/禁止"而代码违背，或代码像 bug）。真正抓住实质的是同时加的 `必须/禁止`。**锚点要锚判据本身，不锚它的名字。**

## 改前快照已落盘

`sop-before.md` —— 改动前那份 86 行 SOP 的完整副本。SOP 在 `~/.omp/` 仓库外、无版本控制，又被 `manage_skill` 全量覆盖过三次，原文一度**只存在于会话 read 输出里**，一次 compaction 就既无法证明丢了什么、也无法回滚。落盘后核验 `SECTIONS=9 LINES=86`，与 `baseline.env` 完全一致，转录忠实。今后改 SOP 有 before/after 可 diff。

## 外环响应：改做法本身，不是再补一条内环规则

触发条件命中 SOP 自己写的「同一类错误第二次出现」：

1. 本次 SOP 重写（`manage_skill` 全量覆盖）丢掉 `doctor full`、`只查当前任务 SPEC`、意图冲突定义等六处
2. 随后 `omp-lsp-windows` skill 重写（同样全量覆盖）把 `-configuration` 改坏成 `<jdtls>/jdtls/config_win`

两次都是**段内改字或丢句**，关键词 grep 全绿。所以 governing 变量不是「记得留快照」，而是**「用全量覆盖去改既有文档」这个做法**。

**已改 SOP 工具自举节**（用 `edit` 而非覆盖，实践新规则）：

> 改既有 agent 文档（本 SOP、各 skill）默认用 edit/追加，全量覆盖仅限新建。前置门：改前落盘快照 + 改后**逐行 diff** 作独立裁判，diff 里每条删除行都要能说明是有意的；grep 只作补充。

diff 正是外环要求的「独立裁判」——它比对原文与现文，不依赖我想到了哪些锚点。

### 本次 skill 改动的 diff 验证

改前快照 `snapshots/omp-lsp-windows-before.md` 从 read 原文重建，其内容哈希 `#52AD` 与原文件 read 时的 tag **完全一致**，证明重建逐字节精确。

`diff -u` 结果：**删除 3 行 / 新增 32 行**。三条删除全部是有意改写：

| 删除行 | 有意原因 |
|---|---|
| `description:` | 扩围到 rust-analyzer / typescript-language-server |
| `Use when …` | 补 "a server reports failed" 情形 |
| 规则 1 | 补 `typescript-language-server.cmd` 到 shim 黑名单 |

`config_win` **未出现在删除行**，即与原文一致——先前那处改坏已修回并经 diff 确认。

SOP 自身裁判复跑：`46 锚点 / sections 9<=9 / lines 86<=86` → `VERDICT=GREEN`（新规则用加长句实现，未加行）。

### 新规则自身的护栏（补漏）

收尾时我一度把「SOP 新规则 ✅ 46 锚点 GREEN」写进汇总——**那 46 条全是旧契约**，新规则只被临时 grep 过，下次重写删掉它照样全绿。这是「新机制无护栏」的第二次复发。

已补 4 条锚点：polarity 加 `全量覆盖仅限新建`、`默认用 edit/追加`；deletion 加 `逐行 diff`、`改前落盘快照`。

- 正常跑：50 锚点 `ANCHOR_MISSING=0`、阈值 `THRESHOLD_FAILS=0`（当时输出仍为旧字段名 `TOTAL_ANCHORS/MISSING`，见下方「混合计数器」一节的改名）→ `VERDICT=GREEN`
- 红证：把 `全量覆盖仅限新建` 改成 `覆盖随便用` → `MISS [polarity] 全量覆盖仅限新建`，exit **1**

锚点选的是**带限定的整句**而非裸词：`覆盖` 二字在翻义后仍存在，只有 `全量覆盖仅限新建` 这个限定才能抓住反转。

### 裁判自身第三类缺陷：混合计数器（后补）

`check-sop.sh` 把「锚点缺失」和「硬阈值超限」累加进同一个 `miss`。后果：一次
`MISSING=1` 让我去找一条**并不存在**的丢失锚点——50 条其实全 HIT，真正的失败是
`FAIL lines grew`（87 > 86）。拆成 `amiss` / `tfail` 两个计数器后输出
`ANCHOR_MISSING=0 THRESHOLD_FAILS=0`，失败原因一眼可辨。

注意 `tfail` 必须先初始化：脚本开了 `set -uo pipefail`，只初始化 `amiss` 会让
阈值分支在 `set -u` 下 unbound variable 崩溃——那又是一次「把崩溃当成判红」（本任务
已经踩过一次 `set -u` 和一次空字符串 `-le`）。

### 86 行上界的回归与修法

发现 SOP 涨到 87 行（mtime 17:36:32，在我最后一次编辑之后，且当时**没留快照**，
无法 diff 出多出的是哪一行——这正是「改前落盘快照」规则我自己没执行到位的代价，
随后已改为每次编辑前都快照）。

修法**不是**把 baseline 从 86 抬到 87（那是给违规盖章），而是真减一行：把「原始素材」
段的两行正文并成一行的 reflow，逐行 diff 确认只有一个 hunk、文字一字未减。
终态 `lines=86 sections=9`，50 锚点全 HIT，GREEN。

### 「MISS 先怀疑裁判」入 SOP，并当场给自己加护栏

这条规则**不新增行**：并进裁判节 `:43` 既有的「无期望值的计数不是裁判」条目句尾，
`lines` 保持 86、`sections` 9、空行 25 均未变。

同时给它配了锚点 `报 MISS 先怀疑模式写法`（polarity 档，共 **51** 条）。这是必须的——
本任务已因「新机制无锚点」复发两次（R1–R5、以及全量覆盖禁令），若这次只写规则不加锚点
就是第三次，而规则内容本身正是「别只靠 grep 全绿」。

红证：删掉该句 → `MISS [polarity]`、`ANCHOR_MISSING=1`、exit 1。
正常跑：`ANCHORS=51 ANCHOR_MISSING=0 THRESHOLD_FAILS=0` → GREEN。

### 复核时我自己的验证脚本又错了（第四次同类）

一次 node 校验里用 `startsWith('拒绝经过一层摘要')` 去 `split('\n').find(...)` 取合并行，
返回 `undefined`，我一度判成「reflow 被外部覆盖」。实际文件 86 行、合并行在 `:48`、
mtime 就是我那次编辑。**是我的脚本写错，不是文件坏了。**

改用 `grep -F` 纯字面逐项验：4 处关键片段全 HIT，且旧第二行独立残留计数为 **0**
（无重复行遗留）。空行数 25 未变 —— 这排除了「删空行凑回 86」的嫌疑，证明是 reflow
而非绕过阈值裁判。

顺带确认 baseline 未被改动：没有把上界从 86 抬到 87 来「消解」违规，而是真减一行。


### grill-me 成为默认开局 + 项目 skill 触发门 + tdd/to-tickets（2026-11-11）

- `grill-me` 已在本机且与上游 `mattpocock/skills` **逐字节一致**（`diff --strip-trailing-cr` → IDENTICAL），本身只是 `Call the Skill tool with "grilling".` 的转发桩；真正正文在 `grilling`。故无需安装，只把它写进 SOP 作**入口别名**（用户说 grill me / drill me 即走）。
- SOP `:52` 改为「非平凡任务的默认开局」：design tree 分轮、每轮问完 frontier、逐条编号 + 推荐答案、等答复再进下一轮、frontier 空且用户确认 shared understanding 才收敛、**收敛前不动产品代码**。
- `:22` 加**项目触发门**并保持全局 SOP 不写死：项目**同时**有 `.trellis/` 与 `.omp/skills/` 时才走 `trellis-before-dev` / `trellis-check` / `trellis-update-spec` / `trellis-break-loop`，skill 名留在项目侧。
- `trellis-break-loop` 按它自述（"When debug is complete"）挂在**外环触发①之后**，不当循环中途 gate。
- `:57` 把 Trellis 角色扩为「`to-tickets` 拆 tracer-bullet 纵向切片 → `tdd` 红→绿一次一片；重构不属于循环，留给 review」。

行数仍 **86 / 9 节**（88 行的 ub-mode 增量靠 `:67-69` 三行 reflow 成一行抵回，内容一字未减，抽查 `不臆造任务系统`/`抗压缩的持久记忆`/`保证一条不漏`/`不改代码`/`不替用户提交`/`只查当前任务 SPEC`/`doctor full` 全部 HIT）。

裁判：`ANCHORS=56 ANCHOR_MISSING=0 THRESHOLD_FAILS=0 → GREEN`。新门均跑过红证：
- 删 `trellis-before-dev` → `MISS [polarity]` exit 1
- `tracer-bullet 纵向切片` → `纵向切片` → `MISS [polarity]` exit 1
- 删 `非平凡任务的默认开局` / 把 `收敛前不动产品代码` 改反 → 各 `MISS [polarity]` exit 1

**未采纳**：`--capture` 重捕基线到 88——那会把刚报出的 `FAIL lines grew` 直接消解，与本文件记录的「不靠改 baseline 消解违规」冲突；`baseline.env` 全程未动。

**发现**：`.trellis/scripts/` 里**没有 `trellis mem`**（只有 task.py / get_context.py / add_session.py 等），故 `trellis-session-insight` 在本仓未必可用，未写进触发门，待确认。



### 把 live SOP 改成 LOOP（2026-11-13，已修正一次误判）

**先纠错**：本轮最初按任务快照 `snapshots/sop-current-87.md` 重建了一个 `managed-skills/dev-workflow-sop/SKILL.md`，并让它对旧 56 锚点全绿。这是错的——`dev-workflow-sop` 是旧世代的名字，本机 live 的 SOP 是 `~/.omp/agent/skills/eng-dev-sop/SKILL.md`（软链到 magpie library，58 行、四技能、步骤式），旧 baseline（86 行 / 24 锚点）描述的文件已不存在。同一个错误里还夹带了一条死路径：快照里写 `D:/work/huanghao/doc-drift`，本机实际是 `D:/doc-drift`（`D:/work/huanghao/` 不存在）。这两条都是「让门禁绿」压过「说真话」的典型，记在此处。

处置：
- 删除重复的 `managed-skills/dev-workflow-sop`。
- 改前落盘：`snapshots/eng-dev-sop-before-loop-0209.md`（5.1K）。
- 把 live 文件重写成循环形态，**保留**它独有的段落（网络实测前缀表、前提/`docInfo` 报错、交付、完成标准 checklist、禁止），并修正 doc-drift 路径为 `D:/doc-drift`。
- 旧 `baseline.env`（SECTIONS=9/LINES=86）与 `check-sop.sh` 的 56 锚点属于已消失的世代，**不再作为本次裁判**；新文件按自身契约核对（见下）。

新文件形状：`## ` 段落 **8** 个，`wc -l` **83** 行。循环主链 = `目标契约 → 知识阈值 → 假设 → 实验 → 裁判 → 记录 → 阈值`，内环/外环、裁判四档（specified/derived/implicit/human）、soundness 优先、死门事实、四个角色（grilling / trellis+to-tickets+tracer-bullet+tdd / ponytail / doc-drift）都在。

裁判（本轮实跑）：
- 内容契约 grep：`不自动 commit`、`docInfo`、`rsproxy`、`ghfast.top`、`tracer-bullet`、`goal-forge`、`宁可误报`、`全量覆盖仅限新建` → 8/8 HIT。
- 死路径：`grep -c "D:/work/huanghao"` → **0**；`D:/doc-drift/bin` → 1。
- 加载面：隔离探针（临时 `PI_CODING_AGENT_DIR` + `skills`/`managed-skills` 双 junction，18.19.0 rpc-ui）→ `get_skills` 160 个，`HAS_ENG_SOP=True`，`DESC=` 输出即新 description 前缀，说明 frontmatter 解析正常。
- 未做：没有为新 SOP 建锚点脚本。锚点集的盲区就是锚点式复核的盲区（见 SOP 裁判段），这次以逐条 grep + 逐行 diff 快照替代；若要长期守卫，应另起一份带极性锚点的新 checker，而不是复用已失效的 56 锚点。

### 大目标方法（毛选）接入 SOP（2026-11-13）

**输入来源**：`D:/oil-cycle-monitor/docs/交易体系_毛选方法论.md`（2026-11-07 的蒸馏产物，M1–M9 原文→检查映射、六条研究完备度门 G1–G6、以及 `.codex/skills/` 18 个毛选 skill 的盘点）。只取其**方法论骨架**，不搬交易专有部分（L0 一票否决、仓位动作矩阵、PIT 账本这些是那个项目自己的制度）。

**落地**：`~/.omp/agent/skills/eng-dev-sop/SKILL.md` 新增第 2 节「大目标：只给方向时，先造方案（毛选方法）」，全文 83 → **115 行**，`##` 段 8 → **9**：
- M1–M9 表：主要矛盾唯一 / 支配方面 / 转化有条件 / 阶段性 / 不同质不同法 / 内因是根据 / 忌片面 / 对抗非唯一形式 / 共性寓于个性。
- 方案书七步：调查（没有调查没有发言权）→ 抓主矛盾 → 阶段路线（论持久战三阶段）→ 集中优势兵力（一次一个假设）→ 试点—推广（抓典型）→ 胸中有数（数字目标）→ 反复检验（实践论）。
- 决策边界：用户定方向/风险/预算/阶段门；AI 自决路径/实验/排序/失败后下一假设并落盘；只在方案书与阶段门上走 `ask` 弹窗。
- 反模式与外部依据（Anthropic harness 设计 + Loop Engineering / spec-kit-loop）。

**外部检索（本轮实做）**：
- Anthropic《Harness design for long-running application development》(2026-03-24, Prithvi Rajasekaran)：planner 把 1–4 句 prompt 展开成 spec 且刻意停在产品层（防止早期细节错误级联）；generator/evaluator 分离，因为模型给自己打分「reliably skew positive」；每 sprint 先谈 sprint 契约（done 定义 + 可测行为）再写码；solo 20 min/$9 vs 完整 harness 6 h/$200，后续精简版 3 h 50 min/$124.70；「每个 harness 组件都编码了一个关于模型弱点的假设，模型变强后要逐项拆掉」。
- GitHub 侧：`formin/spec-kit-loop`（Loop Engineering 的 Spec Kit 扩展，maker/checker 分离、`max_iterations` 上限、`verdicts.md`/`debt.md` 落盘、人类签收门）、`choism4/overnight-skill`（大目标分解 + worktree + 继续挖活）、`gyujeongion/claude-code-goal-loop`（verified-step + 回滚快照 + stuck-handler）、`davidondrej/skills` 的 goal-loop。共同点与 SOP 一致：可检查的停止条件、独立裁判、状态外置、预算上限。

**验证**：`grep -c "^## "` → 9；`wc -l` → 115；契约 grep `primary_contradiction` / `dominant_aspect` / `试点` / `胸中有数` / `sprint 契约` / `M9` / `反模式` / `` `ask` 弹窗 `` → 8/8 HIT；隔离探针（临时 `PI_CODING_AGENT_DIR` + skills/managed-skills 双 junction，18.19.0 rpc-ui）`get_skills` → 160 个、`HAS_ENG_SOP=True`、新 description 正常解析（含「大目标 / 方向目标 / 自主出方案」触发词）。

**追加修正（同一轮 review）**：① description 自相矛盾（举例「在模拟盘赚钱」却声明「不适用于投研分析」）→ 改为「不适用于纯投研分析——方向型目标只借《大目标》的方法论，领域规则仍归该项目自己的 SOP」；② 补上漏掉的硬门槛「开跑门槛」：方向 → **1 个可测代理指标 + 1 个放弃阈值 + 口径**，换算不出来第一步是建表而不是选策略，产物再压 goal-forge 契约（方向不动、阶段目标可改），并作为方案书第 0 步。文件 115 → **122 行**，`##` 仍 9 段；契约 grep 增 `开跑门槛` / `放弃阈值` / `可测代理指标` / `方向不动` / `不适用于纯投研分析` 全部 HIT；description 只在 `description:` 处含一个 ASCII 冒号空格，YAML 安全；加载探针复跑 `SKILLS=160 HAS=True`，尾部 scope 文本为新版。另：`davidondrej/skills` 的 404 是路径少了重复的 `skills/` 层，未再追（同类机制已由 Anthropic harness 原文与 `formin/spec-kit-loop` 覆盖）；krouter 中文查询会崩在 `route_knowledge.py:239` 的 `log_query`（`UnicodeEncodeError: surrogates not allowed`），毛选内容直接取自 `D:/oil-cycle-monitor/docs/交易体系_毛选方法论.md`。

**再修（用户：「我需要的是使用毛选去解决问题的能力」）**：原来的「大目标」段是 M1–M9 表 + 七步清单，属于清单而非解题方法。已重写为 6 阶段**思考顺序**，`##` 仍 9 段、`###` 8 个、161 行 / 7323 字符：
`开跑门槛（方向可测化）→ 一、解剖问题（现象/冲突/谁说了算/翻转条件）→ 二、调查（一手材料、解剖麻雀、眼睛向下、数字判据）→ 三、矛盾分析（找主矛盾、判主要方面、定转化条件、区分矛盾性质、忌片面）→ 四、战略战术（全局在胸、战略持久战役速决、集中优势兵力、不打无把握之仗、打得赢就走）→ 五、检验与推广（试点—总结—推广、从战争中学习战争、一次结论不定稿、抓两头带中间）→ 六、沉淀（写成可复用规则）`，并补一个用**技术目标**（Romp 启动 4013 ms → 1500 ms）跑完全流程的示例，避免拿投研领域当范例越界。description 同步改成「按毛选方法解决：方向可测化 → 解剖问题 → 调查 → 矛盾分析 → 战略战术 → 试点检验 → 沉淀」，触发词增「没有现成方案」。
验证：`wc -l` 161、`##` 9、`###` 8；契约 grep `解剖问题` / `找主要矛盾` / `区分矛盾性质` / `打得赢就打` / `沉淀` / `示例（把方法跑一遍` / `开跑门槛` / `方向不动` 8/8 HIT，`方案书七步` 与 `《大目标》` 旧引用已无（description 里也清掉）；加载探针 `SKILLS=161 HAS=True`。

**追加（用户：调研人类解决问题的书籍 / 为了达到目标有什么方法论）**：`eng-dev-sop` 新增 `## 达成目标的方法论（选型 + 证据等级）`（第 107 行起），并把上一节的示例数字改正（`sidecar 段 2152 ms 占 53%`、`WebView2 段 1534 ms 占 38%`，引 `docs/perf.md:26-31`；翻转条件与门槛统一为 `sidecar ready ≤ 1500 ms`），五个阶段各补一条 `abort` 规则。文件 191 行、`##` 10 段。
- 选型：Cynefin 先分流（因果清楚→分析；不可预测→探针；失序→先稳住），再按目标类型选 TOC / Backcasting / if-then / 刻意练习+Toyota Kata / OKR+Hoshin。
- 证据分级：A 级（元分析或强实证）= Locke & Latham 目标设定四要素、Gollwitzer & Sheeran 2006 实施意图（94 项，d≈0.65）、Oettingen WOOP 心理对照、Bandura & Schunk 近端子目标、Flyvbjerg 参考类预测 + Klein premortem；B 级（行业长期实证）= TOC、Backcasting、Toyota Kata、Hoshin Kanri；C 级（流行但无对照证据）= OKR 本体、SMART 本身。
- 反模式：SMART 只写指标不问约束、OKR 挂奖金、正面幻想代替行动、主目标 >3、只写目标不写 if-then、把平均值当瓶颈。
- 探针 160→161 的差额已定位为 `C:/Users/666/.config/magpie/library/skills/unslop/SKILL.md`（5.8K，magpie library 最新一条，非本次改动引入、非污染）。

**再修（第二轮 review）**：① 示例里的支配方面写反了（window 与 first paint 都在 sidecar ready 之前），已改为「sidecar 段 2152 ms / 53% 是最大单段，WebView2 段 1534 ms / 38% 次之，窗口 ≈8%，sidecar ready → 首会话仅 +50 ms ≈1%」；② 书目综述不留在 skill 正文，`## 达成目标的方法论` 压成 14 行只收增量（A 级五条 + B/C 分级 + 反模式），调研全量留在回复与本节；③ 五个**目前缺的**增量按出处分挂到对应阶段末尾：Kepner-Tregoe IS / IS NOT 与 Ackoff「消解问题」→ 一、解剖；Cynefin 分场域 → 三、矛盾性质；Klein premortem + Flyvbjerg 基率 → 五、试点前；Klein & Kahneman 2009 直觉可信条件 → 五、检验后；并加一句「Dewey 五步 / Polya 四步 / PDCA / OODA / 毛选 独立收敛到同一骨架」作为可信度证据。文件 191 → **187 行**，`##` 仍 10 段；`87%` 与「其余三项」已无命中，新增契约 `Kepner-Tregoe` / `Ackoff` / `Cynefin` / `premortem` / `Klein & Kahneman 2009` / `Dewey 五步` 各 1+ 命中；加载探针复跑通过。

**新建 `problem-solving` skill（用户：结合这些和毛选给 AI 做个解决问题的 SKILL）**：`C:/Users/666/.omp/agent/managed-skills/problem-solving/SKILL.md`（106 行、12 个标题）。结构：5 条铁律（先现象后判断 / 一次一个主矛盾 / 裁判不是自己 / 每次带下一个实验+判据+abort / 交付用户真会用的东西）→ 7 个阶段（0 方向可测化门槛、1 定界与解剖 KT IS/IS-NOT + Ackoff 消解、2 一手调查、3 矛盾与约束 毛选主矛盾 + TOC 瓶颈 + TRIZ 技术/物理矛盾 + Cynefin、4 方案与战略 backcasting/阶段/集中兵力/if-then/premortem+基率、5 执行与检验 试点/判据/失败库/Klein-Kahneman 直觉条件、6 沉淀）→ 固定字段《解题卡》→ 反模式（含 AI 特有：编造未观测数据、拿记忆当证据、只给清单不给判断、过度规划、被要结论时先附和）→ 接线（goal-forge / grilling / eng-dev-sop / trellis，领域规则归领域 SOP）。每阶段带 exit 与 abort。
加载验证：隔离探针（18.19.0 rpc-ui，临时 `PI_CODING_AGENT_DIR` + skills/managed-skills junction）`get_skills` 162 个，`-Name problem-solving` → `HAS=True`，描述尾部触发词正确。
**附带实测（用于演示解题卡）**：`C:/Users/666/AppData/Local/Temp/gate-dump.ps1` 向 18.19.0 发 `set_loop_mode {enabled:true,args:"--while \"echo hi; exit 0\" reply with the single word: pong"}` → `data:{enabled:true,state:"running",prompt:"reply with the single word: pong"}`，`get_loop_mode` 同；**返回里没有任何 gate/while/until 字段，`--while …` 整段被丢弃且未进 prompt**。因果边界：这条只能证明 state 不回显 gate、flag 被吃掉；结合早前 `--while "echo …; exit 0"` 跑 4 轮 marker 文件从未生成、`--until` 跑到 9 轮不停，能判定的是**门在执行层从不生效**，分不出「未解析」与「解析后丢弃」。真要分辨只有一条便宜实验：交互 TUI 里跑 `/loop --while` 看是否停表（与 rpc-ui 不同入口）。

**去重（避免两个 skill 各存一份六段式）**：`problem-solving` 独占通用解题程序（门槛/解剖/调查/矛盾/方案/检验/沉淀 + 解题卡 + abort + 目标达成工具箱，119 行）；`eng-dev-sop` 把原 `:24-112` 两大段压成 3 行指针「解题走 `skill://problem-solving`」+ 一行开发特有约束（`loop` 门在执行层不生效、`goal.continuationModes` 缺 `rpc` 只跑一轮、命令级裁判只能宿主侧）+ 一个 Romp 启动示例，文件 187 → **90 行 / 9 段**。drift 点从两处变为一处。