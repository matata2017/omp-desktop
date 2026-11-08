# Implement: sidecar 成本、上限与生命周期

## 阶段 0：文档与规范漂移（先做，独立提交）

- [ ] 用 `doc-drift` 技能收口（`init` / `doctor` / 对 README、architecture、parity、performance-measurement 做 `audit`）；不手写推断，范围外漂移只列不改。
- [ ] README:25 / README:60 / docs/architecture.md:146 / docs/parity.md:210 的默认值统一为代码实际值 6，并标注“cap 3 行为待有效对照（11-08）”。
- [ ] 修掉预算公式的口径混用：WorkingSet 与 Private 不得相加；改为两套并列或单一口径。已测参考：Private 每 sidecar 489–542 MB（N=3），固定部分约 388 MB（单样本待复核）；RSS 历史值 480 + 240/tab 单独标注。
- [ ] `.trellis/spec/guides/performance-measurement.md`：:30/:38 的 231/240 MB 标注为 WorkingSet 口径；:34 的 `CREATE_NO_WINDOW` 清单与实测 `conhost.exe` 11 MB 对齐（定位漏掉的 spawn 或注明例外）。
- 验证：`grep -rn "默认 3\|默认上限 3" README.md docs .trellis/spec` 为空；`grep -rn "ROMP_MAX_SIDECARS" README.md docs .trellis/spec` 全部为 6 或附说明；`231 MB|240 MB|480 MB|500 MB` 每处都标了口径（WorkingSet/Private）；`doc-drift audit verify` 全绿；`gate.ps1` 在基线 JSON 上 exit 0。
## 阶段 1：量具

- [ ] `measure.ps1` breakdown 增加 `privateMB`（role 与逐进程），WebView2 增加 `--type`。
- [ ] `-KeepRunning` 输出 launched PID；文档注明 finally 杀树行为。
- [ ] `Tabs > 3` 注入 cap 时打印实际生效值。
- [ ] BOM-free layout helper；resume 探针以 `resume=Some` 为通过条件。
- 验证：`-Tabs 1 -NoDrive -KeepRunning` 输出含 `privateMB` 与 PID；`gate.ps1` 兼容。

## 阶段 2：cap 决策（有效性优先）

- [x] 驱动：表达式写入无 BOM 的 UTF-8 文件，用 `cdp-eval.mjs @file` 传入（PowerShell 5.1 的 argv 引号会吞反斜杠/双引号）；顶层 `await` 依赖 `replMode`。
- [x] 隔离：`PI_CODING_AGENT_DIR=<temp>`（`OMP_DATA_DIR` 不隔离 sessions），并复制 `models.yml`/`models.db*`/`config.yml`/`agent.db*`；排除 `probe-*`，3 个源文件按第 2 行 `session.id` 断言互异；副本只放 temp，结束清理。
- [x] 有效性门槛：`stable=true` 且 `spawns≤6`、`exits≤1`；**不要轮询 `tabs.list`**（`tab:get-all → ensure_tab_started` 会把一次退出放大成风暴）。矩阵落 `docs/perf.md` 与 research §8：cap3 中位 1797.2 MB（1 tab 不 ready）、cap4 2347.3、cap6 2352.7，默认保留 6。
- [x] 待查 bug 已归因：被驱逐 tab 停在 `starting` 是 `tab:get-all` 自动拉起 + cap 驱逐的交互；去掉轮询后 `exits≤1`。产品层面是否需要稳定 `asleep` 状态仍待决定。
- [ ] 切 tab 首帧延迟（cap 3 vs 6）各 10 次，median/p95。

## 阶段 3：启动归因

- [ ] N=10 采集四段里程碑（window / first-paint / sidecar-ready / first-session）median/p95。
- [ ] WPR `.etl` 一次，与 app mark 对齐；两通道差 < 50 ms。
- [ ] 选一个候选做改动；前后各 N=10，p95 置信区间不重叠才算通过。
- 验证：命令 + 原始 JSON/ETL 路径；未达标则记录证据并回滚。

## 阶段 4：生命周期

- [ ] 100 次 open/close（每 10 次采样 private/RSS/句柄/线程），回归斜率 + 95% CI。
- [ ] 1 小时 streaming（固定间隔），记录 sidecar private 斜率与 transcript 增长。
- [ ] 正常退出 + 强杀两种路径的孤儿计数；目标 0。
- [ ] 定位 `conhost.exe` 的 spawn 点，确认与 spec :34 的例外/遗漏一致。
- 验证：斜率 CI 跨 0 视为无泄漏；孤儿计数为 0 或附缺口说明。

## 阶段 5：agent runtime 文档

- [ ] `docs/agent-runtime.md`：hash/版本/大小/来源、~500 MB private 基线、能力探测、本仓库可控 vs 不可控、预算与 cap 的关系。
- 验证：文中每个数字都能指向 `.perf/*.json` 或一条命令。

## 全局验证

- [ ] `cargo test --workspace`（176 passed 基线）、`bun run test`（1589 passed 基线）、`bun run typecheck`。
- [ ] `bun run perf:gate` 在基线 JSON 上 exit 0。
- [ ] `git status` 干净；文档漂移与 instrument 改动分开提交；不自动 push。
