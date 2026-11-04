# Implement：Core tabs and sessions

1. `omp-session`：JSONL 索引、标题槽、头尾扫描、搜索缓存、跨会话 SQLite 索引。
2. `SidecarPool`：活动路由槽位、焦点 warm 租约、失焦宽限、释放开关、并发上限、`--resume` 恢复。
3. tab 生命周期：创建/关闭/激活/分屏/persist/restore；per-tab batcher。
4. 会话操作：切换/重命名/删除/归档/搜索/树/branch/fork/export。
5. 会话周边：info/handoff/share/import/workspace/model-restore/picker。
6. parity 账本逐行推进；每批跑测试。

验证：`cargo test --workspace`、`bunx tsc --noEmit`、窗口烟测（开 3 tab，切回已释放 tab）。

回滚：按子批次提交；释放开关可关。
