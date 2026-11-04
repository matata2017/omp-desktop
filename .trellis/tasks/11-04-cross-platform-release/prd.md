# PRD：Cross-platform release（post-1.0，延期）

父任务：`11-04-full-replication`

## 状态

**延期**。当前交付只做 Windows x64；本任务在 Windows 1.0 发布后启动，或用户明确要求提前。现在只保留平台扩展点，不投入构建/签名/CI。

## 目标（post-1.0）

让 Romp 在 macOS（arm64 + x64）与 Linux x64 上构建、安装、起窗并完成核心回合；Windows 已有 NSIS，不再重复。

## 范围

- macOS：`.app`/DMG、Tauri WKWebView、`omp` (darwin) sidecar、`omp://`、unsigned 默认；Apple Developer ID 就绪后签名/公证。
- Linux：AppImage/deb、WebKitGTK、`omp` (linux) sidecar、`omp://`；文档声明 WebKitGTK 运行时依赖。
- 平台差异：托盘/菜单/窗口状态/深链/终端 PTY；浏览器面板在 macOS/Linux 走外部 Chrome/Chromium CDP relay，嵌入式 WKWebView/WebKitGTK 只承载 UI。
- CI 矩阵：macos-14、ubuntu-22.04+；Windows 已有。
- sidecar：按平台获取/构建并校验 SHA256 与 `--mode rpc` 命令面。

## 前置

- Windows 1.0 的 133 条功能全部 `done`，架构中的平台适配层已抽出。
- macOS/Linux sidecar 二进制来源确定。
- 签名凭据（可选）：Apple Developer ID / Windows 代码签名证书；无证书交付 unsigned。

## 验收（post-1.0）

- macOS/Linux `tauri dev` 起窗并完成核心回合；`tauri build` 产出 `.app`/DMG、AppImage/deb。
- 两平台 CI 全绿；安装包可安装、可起窗、可完成核心回合。
- `omp://` 深链、托盘/菜单、终端、浏览器 relay 在两平台可用。
- 文档包含 WebKitGTK 依赖与 unsigned 安装提示。
- `docs/parity.md` 的 packaging 行在 Windows 已 `done`；跨平台只补平台证据，不新增功能行。

## 不在范围

Windows 功能实现（已在 1.0 完成）；移动端。

## 开放问题

签名凭据（非阻塞，unsigned 可交付）。
