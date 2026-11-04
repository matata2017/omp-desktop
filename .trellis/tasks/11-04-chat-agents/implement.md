# Implement：Chat and agents surface

1. 核心回路：prompt/stream/tool/agent_end；事件路由与批处理。
2. 输入：steer/follow-up/abort/queue/dock。
3. 渲染：markdown/代码/KaTeX/Mermaid/thinking/usage。
4. 交互：extension UI/审批/ask/plan approval。
5. 代理：Hub/roster/page/beside/hover/BTW。
6. parity 账本逐行推进；每批跑测试。

验证：完整回合烟测、steer/follow-up/abort、审批与 ask、子代理页。

回滚：按功能族提交；renderer 不改。
