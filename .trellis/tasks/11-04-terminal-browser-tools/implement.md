# Implement：Terminal, browser, tools, PR, memory

1. `omp-terminal`：portable-pty、多 shell、seq 对齐、链接/CLI。
2. `omp-browser`：子 webview、CDP relay、持久分区、休眠/唤醒、链接路由。
3. 工具卡 registry：补齐三组 renderer 与 Generic 兜底。
4. PR Center：gh 探测、列表/详情/diff、AI 起草、checkout worktree。
5. Memory 窗口：knowledge/skills/sources、项目选择、纠正/遗忘。
6. parity 账本逐行推进；资源迁到 Rust 后才开 sidecar 释放。

验证：终端/浏览器/工具卡/PR/Memory 烟测；sidecar 释放后终端与浏览器仍可用。

回滚：按表面提交；浏览器/终端可用开关关闭。
