# DSH 深入重构：方法与证明

状态：研究方案。没有测得 DSH 加速倍数，没有确认运行瓶颈。用户批准规划，尚未批准实施。

## 结论边界

Rust 是实现候选。端到端目标是固定正确结果下更少的等待、计算、复制与常驻状态，不是提高 Rust 代码占比。Safe Rust 的内存安全不等于业务正确、不会死锁、不会泄漏或自动更快；unsafe/FFI 另有契约。

- Rust Reference（资源泄漏不属于 unsafe）：https://doc.rust-lang.org/reference/behavior-not-considered-unsafe.html
- Rust Book（unsafe 的额外义务）：https://doc.rust-lang.org/book/ch20-01-unsafe-rust.html

若当前关键路径有比例 p 可优化，该段加速 s 倍且其他工作不变，则整体加速比为 1 / ((1-p)+p/s)。假设 p=10%，该段快 10 倍，整体约快 1.10 倍；无限快也只约 1.11 倍。这是数学示例，不是 DSH 测量。并行/流水线场景必须用关键路径而不是累加所有 span。

## 专家额外检查的内容

1. 先冻结目标：启动到可操作、打开长会话、流事件到可见帧、取消到子进程退出、峰值与稳态内存、正确任务完成耗时。模型首 token 与本地首帧分开。
2. 检查原始身份：源码提交、二进制哈希/版本、构建参数、OS/硬件、配置/插件集。现有本地源码 HEAD 687f282e8625c6b8d11ff58211a90524be2e4317 与更新脚本版本不一致，不能混成一条基线。
3. 建立状态所有权表：谁拥有 session、transcript、工具子进程、订阅、pending approval、取消信号与持久化提交。检查同份数据存在多少副本及销毁条件。
4. 排除多做的工作，再比较语言实现：全量重放是否可按需读取，重复进程是否可复用，无界队列是否应施加背压。以上均是假设，需要 trace 验证。
5. 分解收益：现有语言结构优化、Rust 热路径、Rust 核心、全量重写四个候选分开；避免把算法改变的收益记到语言上。
6. 预先约定否决条件：错误率上升、取消失效、丢数据、必要插件无法运行，或收益不足以覆盖迁移成本，则候选失败。

方法参考：Brendan Gregg 的 USE（资源利用率、饱和度、错误）与时间分解/Off-CPU 方法。先问资源与等待的问题，再选工具。
https://www.brendangregg.com/usemethod.html
https://www.brendangregg.com/methodology.html

## 原始材料顺序

- 正在使用的可执行文件、原始入口/构建脚本/锁文件、原生模块与插件实际导入路径。
- OS 进程树及生命周期事件、CPU 栈、阻塞/唤醒、文件 I/O、网络时间点；不以安装包大小替代 RSS/private commit。
- 原始流帧与工具事件，记录 turn/session/request/sequence ID；先用脱敏公开 fixture，私人数据只采最小必需字段。
- 堆对象保留链、分配次数/字节、字符串/Buffer/JSON 复制；观察重复开关会话之后是否保留增长。
- 落盘记录、截断记录、取消时序、子进程崩溃、重连边界；先定义正确结果，再看实现。
- 上游 benchmark 的输入、计时起止与断言。文档/旧研究用于找入口，不替代上述证据。

## 工具选择

- Windows WPR/ETW + WPA：CPU、磁盘、内存和 UI 响应时间线。https://learn.microsoft.com/en-us/windows/win32/etw/event-tracing-tools
- Node/V8 CPU profile、heap snapshot 与 Chrome DevTools：JS 热点、对象保留；heap snapshot 会停顿并增加内存，应在隔离测量运行中采集，不能用采样运行当最终速度成绩。https://nodejs.org/en/learn/diagnostics/memory/using-heap-snapshot
- Rust release build + 符号、samply；必要时分配分析：验证真正热点而非猜测。https://nnethercote.github.io/perf-book/profiling.html
- 确定性事件回放与独立输出判定器：同一输入比较原实现和候选的用户可见行为。旧实现不等于真理，已知旧错误以明确契约裁决。
- Loom：小型并发核心的交错探索；Kani：适合抽出的 Rust 核心性质。仅在适用环境/边界使用，不能据此宣称整个产品被形式化证明。https://github.com/tokio-rs/loom https://github.com/model-checking/kani

## 独立证据协议（拟议）

1. 预注册场景、计时边界、硬件/电源模式、成功标准、排除规则和收益门槛；不得跑完再挑指标。
2. 两条跑道：确定性录制流/工具结果回放隔离本地开销；真实模型+真实工具任务复测体验和正确完成率。固定模型名不保证模型确定性，记录请求/响应与服务版本能取得的全部信息。
3. 成对随机交错 A/B，分开冷启动与热态，用多批次重复；基于先导方差决定样本量，不用几十次估计可靠 p99。报告 p50/p95、置信区间、失败和超时，不能只报最快一轮。
4. 进程树级内存：分别报 private commit、working set、JS heap；共享页不能直接求和当物理占用。记录峰值、静稳态及多轮增长斜率，固定 idle 定义。
5. 行为不变量：会话不串线；每个请求唯一终态；已确认持久化的数据恢复后存在；取消后的新副作用被禁止或明确归入取消前已提交部分；重连不遗漏/重复应用事件；队列有界；资源在生命周期结束后释放。
6. 故障注入覆盖取消/完成竞争、流切断、子进程异常、落盘半途崩溃。外部副作用无法笼统承诺 exactly-once，应定义幂等性和已提交边界。
7. 独立复现包：固定提交与构建哈希、公开/脱敏输入、命令、原始 trace/CSV、分析程序、结果判定器及失败案例。另一台机器/独立执行者按包重跑；审阅者可尝试反证。
8. 只有小核心能在明示假设下作更强证明；测试、模型检查、OS trace 各证明不同事情，不能相互替代。

## 本轮实测状态

读取了本地源码版本与文件。通过 PowerShell 按 ProcessName 含 DeepSeek 检查进程，调用成功但无匹配；这不排除改名进程，因此没有运行中版本或内存成绩。未启动应用、未跑产品基准、未读取用户私人会话。后续必须先绑定可执行产物才能开展性能判断。
