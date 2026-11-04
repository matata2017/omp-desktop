# Implement：System, scheduler, packaging

1. 菜单/托盘/深链/单实例/窗口几何/退出守卫。
2. 系统集成：通知、对话框、剪贴板、openExternal、录制落盘。
3. 命令面板、sidecar 横幅、诊断面、/tools /jobs /force /copy、反馈、benchmark。
4. 定时任务：store、headless runner、runs/review/insights、通知铃。
5. 更新状态机与 GitHub releases 源。
6. Windows 打包：图标、NSIS、`resources/omp.exe` 映射、`omp://`、许可清单。
7. parity 账本逐行推进。

验证：托盘/深链/退出守卫；定时任务 headless；`bunx tauri build` + NSIS 安装包烟测。

回滚：打包保留上一版；更新失败不自动替换；按域提交。
