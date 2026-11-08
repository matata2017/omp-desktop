# PRD: 发布版性能基线与深度测量

## 1. 问题

`docs/perf.md` 记录的所有数字都来自 dev 构建，发布版从未测过。由此带出五个具体缺口：

1. 两个测量脚本并存。`scripts/measure-perf.ps1` 会等 `runtime.log` 的 `sidecar ready` 并开 CDP 端口；新的 `scripts/perf/measure.ps1` 丢了这两个门，还丢了 tab 驱动。两边的 `-Tabs` 都只是回显，没有真的开 tab。
2. 冷启动只测到窗口句柄出现（137 到 321 ms）。真正贵的是 sidecar ready 的 2 到 4 秒和首会话渲染，这两段没有数。
3. 预算是按 dev 数字写的：①启动空闲 120 MB、②单 tab 250 MB、③4 tab 500 MB。dev 实测是 883 MB 和 1273 MB。预算必须按发布版重定。
4. sidecar 死后 UI 仍显示 ready，命令全部超时。工作区已有一版 watchdog，未编译、未验证。
5. `omp stats` 在 setup 里预启动，常驻 96 MB；强杀主进程会留下孤儿。两者都有现成修法，但都没有数字证明修完有效。

界面侧同样没有工具账本：三个 locale 各约 4200 行，`en.ts` 有 381 处破折号；从没跑过可访问性检查，也没有真实窗口截图基线。

## 2. 目标

G1 一个测量器。删掉重复脚本，只留 `scripts/perf/measure.ps1`，输出机器可读 JSON。
G2 发布版基线。用 `target/release/romp-desktop.exe` 测冷启与进程树内存，写进 `docs/perf.md`。
G3 上游对照。用同一脚本测上游 Electron 版，给出“Rust 到底快多少”的分母。
G4 启动分段。窗口、首帧、sidecar ready、首会话渲染四段各有数字。
G5 进程模型决策。per-tab 与共享 sidecar 的对照实验，采纳或否决都要有数字和理由。
G6 流式渲染热点。一次真实流式回答的 CDP profile，指出主线程花在哪。
G7 UI 审计。文案命中、axe 违规、截图三份证据。
G8 预算门。超预算或回归超过阈值的脚本以非零码退出。

## 3. 非目标

不加功能，不重做视觉，不做跨平台，不替换 renderer，不改 sidecar 协议（除非 G5 的实验通过）。

## 4. 验收标准

A1 一条命令产出发布版基线：
`powershell -NoProfile -File scripts/perf/measure.ps1 -Exe target/release/romp-desktop.exe -Label release-idle`
输出含 `coldStartMs`、`milestones`、`treeRss`、`breakdown`。连跑两次，冷启差超过 25% 时必须记录原因。

A2 同一脚本测上游 Electron 产物，产出对照表。构建失败时记录可复现的错误日志路径，对照降级为发布版自比，不许口头断言。

A3 四段里程碑都有数，且每段能指出来源（Rust 日志、renderer mark 或 CDP）。

A4 共享 sidecar 实验给出三组数字和明确结论。否决时写清协议原因，并记入 `docs/architecture.md`。

A5 流式回答的 profile 给出主线程 `TaskDuration`、`LayoutCount`、`RecalcStyleCount` 和 top self-time 函数。若改动 `ChatStream.tsx`，必须有改动前后同口径对比。

A6 UI 审计报告落盘，含按使用频次排序的文案命中、axe violations、截图。高危项清零或逐条记录理由。

A7 gate 脚本在人为超预算的 JSON 上 exit 1，在基线 JSON 上 exit 0。

A8 所有数字可由仓库内命令复现，`docs/perf.md` 明确标注构建类型（release/dev）与测量日期。

## 5. 约束

renderer 沿用上游，字节级修改只允许 `main.tsx`、`index.html`、`ChatStream.tsx`、`use-session-switch.ts`。修改 locale 属于新增分叉，必须登记进 `docs/handoff.md` 的分叉清单。

Windows 优先。新增依赖只允许 devDependency，并附许可说明。测量脚本不能变成第二套应用。

一切预算以发布版为准，dev 数字只做相对比较。

## 6. 未决

- 上游 Electron 能否在本机构建（网络与依赖）。
- 共享 sidecar 是否被 RPC 协议否决：命令不带 sessionId，进程内只有一个活动会话。
- 发布版数字出来后才能确定 gate 的预算值。
