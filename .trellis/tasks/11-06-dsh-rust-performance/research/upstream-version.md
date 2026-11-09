# 上游版本核对

2026-11-06 用户指定 https://github.com/deepseek-ai/deepseek-harness 。直接读取 master/package.json 得到：

- DSH package version 0.2.2-rc.1。
- devDependencies.typescript 声明 ^6.0.3。
- engines.node 声明 >=24.9.0。

来源：https://raw.githubusercontent.com/deepseek-ai/deepseek-harness/master/package.json 。master 是移动引用，以上是读取时快照；正式基准仍需固定远端 SHA/构建哈希。依赖声明不等同于用户已安装版本。

本地 D:/work/huanghao/dsh-src/package.json:283 同样声明 ^6.0.3，而本地包版本 0.2.1-rc.2、HEAD 687f282e8625c6b8d11ff58211a90524be2e4317。源码审阅结果必须标注此旧提交。

TypeScript 官方 Handbook 的 Erased Types 节明确类型标注被擦除，不改变运行时行为。编译器版本和生成目标可能影响构建/产物，但仅凭 TS5/TS6 不能推出运行时瓶颈。
来源：https://www.typescriptlang.org/docs/handbook/2/basic-types.html#erased-types

Rust Reference 明确 safe 代码仍允许死锁、内存及其他资源泄漏、逻辑错误。故 Rust 内存安全不能替代 session 隔离、恢复、取消及资源有界性验证。
来源：https://doc.rust-lang.org/reference/behavior-not-considered-unsafe.html
