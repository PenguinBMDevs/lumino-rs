# EnderDebugger (Rust)

这是一个将原始 C# EnderDebugger 库重写为 Rust 的实现。

主要功能：

- EnderLogger（单例）: 写入主日志文件（EnderDebugger_yyyyMMdd_HHmmss.log），并写入 Viewer 兼容的 JSON 日志（LuminoLogViewer_yyyyMMdd_HHmmss.log 和 LuminoLogViewer.log）。
- 日志级别：Debug / Info / Warn / Error / Fatal
- LogViewer CLI：读取并格式化 Reader 日志文件，支持级别过滤、搜索词、最大行数与是否跟踪文件（轮询）等选项。

如何构建和运行示例：

1. 构建

```powershell
cd d:\source\lumino-rust\EnderDebugger
cargo build
```

2. 运行示例写入日志

```powershell
cargo run --bin emit_logs
```

 3. 运行日志查看器

```powershell
cargo run --bin log_viewer -- --levels DEBUG,INFO --search 错误
```

说明：默认情况下 `EnderLogger` 只有在启用 `debug` 模式时才会写日志，示例 `emit_logs` 已启用调试模式。

 当启动 `EnderLogger`（例如通过 `emit_logs`）时，它会在 `EnderDebugger/Logs` 目录下创建三个文件：
 - 主日志 `EnderDebugger_YYYYMMDD_HHMMSS.log` （仅 message 文本）
 - Viewer JSON `LuminoLogViewer_YYYYMMDD_HHMMSS.log` （每行 JSON）
 - Viewer 静态文件 `LuminoLogViewer.log` （用于兼容旧工具）
 同时，会创建或更新 `LuminoLogViewer.current` 文件，该索引包含当前 viewer 文件名称，便于其他工具找到最新的文件。

 你可以使用 `--debug` 参数来启用调试日志（与 C# 行为一致）：
 ```powershell
 cargo run --bin emit_logs -- --debug
 ```

 注：为了尽量减少崩溃时日志丢失，库对写入后做了 `flush()` 并调用 `File::sync_all()` 尽量将数据落盘（但并非 100% 防止异常终止）。

 本实现包含插针（Ctrl-C）处理器，当按 Ctrl-C 时尽量 flush 并退出。
