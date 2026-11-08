# PRD: sidecar 成本、上限与生命周期

## 1. 问题

2026-11-08 在安装版（22:00:06 构建）上的原始测量暴露了四个问题。所有数字都注明口径；WorkingSet 与 Private 不混用。

1. **成本主项是 agent sidecar，不是 GUI。** 单个 sidecar private 基线中位 ~501 MB（N=3：505.5 / 501.0 / 489.2 MB；同轮 RSS ~240 MB）。恢复一个 6.92 MB 的真实 session 后 private 中位 537.2 MB（541.8 / 537.2 / 534.2，+36.2 MB，约 transcript 的 5.2 倍，两组区间不重叠）。
2. **数量是唯一可控的大杠杆，但默认值自相矛盾，且现有 cap3 样本无效。** 代码默认 6（`ipc.rs:1481 unwrap_or(6)`），README/architecture/parity 旧文档写 3；`ab4cf98` 引入时就是 6，无测试钉住。已做的 cap3 两轮均为 `readyTimedOut=true` + `driveErr=cdp timeout while driving tabs`，只起 2 个 sidecar，且 `resumed=True` 与 cap6 的 `resumed=False` 条件不一致。`performance-measurement.md:20` 明确“启动期导航会关掉 CDP socket，要重试并重连”，因此该样本**不能**作为“cap3 撑不住 4 tab”的证据。cap 取值是“用户能同时要几个 ready tab”的产品取舍，待有效 A/B。
3. **启动只被度量了一小段。** window 297 ms、first paint 1606 ms、sidecar ready 3860 ms、first session 3887 ms；`perf:gate` 只卡 `coldStartMs`（窗口可见）。
4. **量具本身会骗人。** `treeRss` 是 WorkingSet 求和（同刻 RSS 801.5 vs Private 944.1 MB，差 143 MB）；`measure.ps1` 默认在 `finally` 杀树，不加 `-KeepRunning` 无法二次采样；PowerShell 5.1 `Set-Content -Encoding utf8` 的 BOM 会让 `tab-layout.json` 静默回落 fresh tab（resume 探针曾因此无效；`performance-measurement.md:20` 的 CDP 掉线也解释了 cap3 的 drive 失败）。旧文档的预算公式还把 WorkingSet 的 480 MB 与 Private 的 500 MB 混在一条式子里。
5. **规范自身在漂移范围内。** `.trellis/spec/guides/performance-measurement.md:30/:38` 的 231 MB / 240 MB 是 WorkingSet 口径；:34 要求每个 `omp.exe` 子进程带 `CREATE_NO_WINDOW`，但实测进程树里仍有 `conhost.exe`（11 MB），需要确认是哪个 spawn 漏了。

## 2. 目标

- G1 **量具可信**：`measure.ps1` 并列输出 Private 与 WorkingSet（按 role 与 `--type`），支持 `-KeepRunning` 二次采样；layout 写入无 BOM；采样与清理分离。
- G2 **上限决策**：用有效的 4-tab 负载（CDP 重连、同一 session 状态、N≥3）测 cap 3 / 4 / 6，记录 all-ready、sidecar 数、private 合计、驱逐与切 tab 代价，给出 tradeoff 建议。文档先对齐代码实际值 6，并标注“cap 3 行为待有效对照”。
- G3 **启动归因**：window / first-paint / sidecar-ready / first-session 四段各有来源（app mark + ETW），至少一项优化用 p95 与不重叠区间证明。
- G4 **生命周期与泄漏**：100 次 tab open/close + 1 小时 streaming 的 Private/RSS 斜率；退出/强杀后孤儿进程数为 0（Job Object 或等价机制）；定位残留 `conhost.exe` 的 spawn 点。
- G5 **agent runtime 说明**：写清 `omp.exe` 的来源（上游 fork 构建、hash、版本）、基线成本、本仓库可改与不可改的边界。

## 3. 非目标

- 不修改 sidecar 二进制或其 RPC 协议；不改 fork/上游。
- 不加功能、不改视觉、不替换 renderer、不引入第二套测量应用。
- 不动用户真实会话与数据；所有实验用隔离 `OMP_DATA_DIR` 与 session 副本。

## 4. 验收标准

- A1 `measure.ps1` 单次输出含 `privateMB`（按 role 与 `--type`）与 `rssMB`，并能在 `-KeepRunning` 下二次采样；BOM-free 写 layout 的 helper 通过 resume 探针（日志出现 `resume=Some`）；`gate.ps1` 继续兼容旧字段。
- A2 产出 4-tab × cap{3,4,6} 矩阵（N≥3）。**有效性门槛**：每轮 `location.href` 固定、连续两次 `tabs.list()` 成功、`resumed` 状态在条件间一致、无 `cdp timeout`；失败样本作废重跑。默认值据此给建议；文档与代码一致。
- A3 四段启动里程碑各有 N≥10 的 median/p95，且至少一段能指到优化前后的非重叠区间；若无可优化项，给出栈/事件证据说明预算已在下限。
- A4 100 次 open/close 与 1 小时 streaming 后，Private/RSS 斜率有数值与置信区间；正常退出与强杀后孤儿进程均为 0。
- A5 `docs/agent-runtime.md`（或 architecture 新增一节）写清 `omp.exe` 的 hash/版本/构建来源、~500 MB private 基线、本仓库可控的杠杆（数量/时机/复用/预算）与不可控项。
- A6 所有结论可由仓库内命令复现；原始 JSON + ETL/trace + build hash + WebView2 版本落 `.perf/`（ignored），报告只引用可复算的数字，并并列标注 Private/WorkingSet 口径。

## 5. 约束

- **口径单列**：Private/Commit 为主，WorkingSet 只做相对比较；任何预算公式只能使用单一口径，或两套并列且各自标注。
- 单样本只能标注 preliminary；结论需 N≥3（中位 + 范围）或给出方差/置信区间。
- 测量前确认无残留 `romp-desktop` 与 `AppData\Local\Romp` 孤儿 sidecar；结束后恢复正式实例。
- 文档漂移按 `doc-drift` 技能处理（先 `init`/`doctor`/`audit`，不手写推断）；改动留在工作区，不自动提交。

## 6. 与 11-06 的边界

- `11-06-perf-baseline-deep` 拥有：单一测量器、发布版基线、上游对照、共享 sidecar 否决、流式渲染 profile、UI 审计、预算门。
- `11-08`（本任务）拥有：sidecar 成本/上限/生命周期、agent runtime 说明、量具口径修正。不重开 11-06 已完成的范围；发现重叠时以 11-06 的结论为准。

## 7. 未决

- cap 默认值：保持 6、改为 3、还是做成设置项并由产品决定“4-tab 并发的期望”。需有效 A/B 后拍板。
- ETW/WPA 是否需要管理员权限、能否在 CI/脚本里稳定采集。
- `conhost.exe`（每个 console 子进程约 11 MB）由哪个 spawn 产生、能否通过 flags 消除。
