# Implement：Cross-platform release（post-1.0，延期）

> 本轮不执行。启动条件：Windows 1.0 发布后，或用户明确提前。

1. 抽取 `src-tauri` 平台适配层：sidecar 命名、托盘/菜单、窗口状态、深链、路径。
2. 获取/构建 macOS/Linux sidecar，记录来源与 SHA256；探针验证 `--mode rpc`。
3. macOS：`.app`/DMG、WKWebView、`omp://`、unsigned；证书就绪后签名/公证。
4. Linux：AppImage/deb、WebKitGTK 依赖、`omp://`；文档安装说明。
5. 浏览器：macOS/Linux 外部 Chrome/Chromium CDP relay；嵌入式 WebView 只承载 UI。
6. CI：macos-14 / ubuntu-22.04+；缓存 Rust/bun；上传安装包。
7. 两平台烟测：窗口、核心回合、托盘/菜单、深链、终端。
8. 签名门：证书就绪后启用；否则标注 unsigned。

验证：两平台 `tauri dev`/`build`、安装包烟测、CI 矩阵。

回滚：保留 Windows 1.0；平台构建失败不影响 Windows 发布。
