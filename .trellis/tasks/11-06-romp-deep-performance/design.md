# Romp 性能研究设计

研究边界是 e2a9289 的可观察行为，不是提出新功能。详见 research/expert-method.md 与三份源码审查。

## 证据分层

1. 用户时间线：进程启动、窗口、首帧、输入可用、首条会话内容、warm/cold 会话切换、取消、退出；重叠 span 不求和。
2. 进程与所有权：shell/WebView2/agent sidecar/stats/scheduler；分别统计 CPU、private commit、WorkingSet、句柄和退出。
3. 数据流：sidecar 帧、batcher、Tauri emit、bridge 过滤、renderer state/hydration；记录次数、字节和丢弃位置。
4. 正确性：session 绑定、switch cancelled 语义、leases、后台完成事件、布局恢复和子进程归零。

## 候选比较

基线 A 是固定版本原实现；B 是修正测量后消除实测重复工作；C 是已证实热点上的 Rust 内部优化；D 是进程/服务生命周期变化；共享 agent 或替代 UI 作为更高风险架构候选。各候选必须同输入、同行为契约、同测量终点比较，分别报告相对 A 与前一候选的增益，不能把删功能、修测量或算法变化记到 Rust 名下。

## 决策门槛

性能改善不能覆盖正确性退化；迁移/隔离风险超过收益则停止；若测量显示候选路径占比过低，按收益上限放弃优化。插件/进程隔离边界由用户决定，源码只能给出成本估计。

## 当前状态

未运行应用，未测量新数据，未实施优化；历史 perf.md 数字仅作待复核的历史记录。
