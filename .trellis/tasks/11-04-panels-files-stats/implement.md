# Implement：Panels, files, stats

1. `omp-vfs`：ignore 文件树、预览、图片、路径解析。
2. `omp-git`：status/changes/diff。
3. Diff 面板四 scope 与 unified/split。
4. Overview/Queue/Context/MCP 面板。
5. `omp-stats`：10 路由、范围、sync、服务托管。
6. parity 账本逐行推进。

验证：无 sidecar 时 Files/Git/Stats 面板可用；diff 四 scope；stats 路由。

回滚：按面板提交；不改 renderer。
