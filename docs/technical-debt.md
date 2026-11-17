# 技术债务备忘

> 给屎山检查器与开发者：本文件只记录当前仍存在的债务与人工门禁；已删除模块和旧 iced 路径不再作为扫描线索。

## 当前待办

### 多 MIDI 压缩包仍自动选择第一个

`src/runner/menu/file/load.rs` 会列出压缩包内的 MIDI 文件，但仍自动加载第一个。临时目录已由加载任务持有并可自动清理；剩余问题是选择交互，而不是文件生命周期。

### Runner 状态聚合仍需持续约束

`src/runner/inner.rs` 聚合较多运行状态。新增功能应优先沿现有子模块拆分，避免继续增加深层链式访问；不建议为追求行数一次性重写 Runner。

### 真实协作服务器与硬件性能测试仍是人工门禁

`.github/workflows/ci.yml` 已覆盖格式、check、Clippy、workspace 测试及无需外部服务器的协作回归。以下测试仍需显式运行：

- `tests/collab_id_sync_local.rs`：需要本地 `lumino-server-rs`；
- `tests/collaboration_full_test.rs`、`tests/collaboration_ui_test.rs`：需要外部协作服务器；
- GPU/性能基准及外部素材测试：按各测试的 `#[ignore]` 说明运行。

Miri、Android 构建和供应链审计尚未纳入 `ci.yml`，应根据独立预算与平台维护成本另行落地，不能将它们视为已由 CI 覆盖。

## SMA 本轮已闭环

| 问题 | 当前实现 |
|------|----------|
| SFTP 接受任意主机密钥 | `crates/network/cloud/src/sftp.rs` 使用 OpenSSH `~/.ssh/known_hosts`，未知或变更密钥均拒绝 |
| 音频启动失败遗留线程 | `crates/audio/gpu-synth/src/audio/playback/start.rs` 使用启动守卫统一 stop + join |
| CPAL 回调释放旧块并记录日志 | 旧块交渲染线程回收，underrun 日志由 stream-owner 限频输出 |
| GPU 回读逐块环境变量、channel 和缓冲分配 | 环境变量使用 `OnceLock`，channel 与回读 `Vec` 容量复用 |
| 旧 `SwappableBuffer` 三缓冲预分配 | 生产通路、统计字段与实现文件均已删除，音符由常驻 `GpuNoteBuffer` 管理 |
| 压缩包临时目录永久保留 | `ExtractedArchiveEntry` 持有 `TempDir`，Runner 加载任务结束后自动清理 |
| 视频 FFmpeg stderr 无上限、完成等待无超时 | 保留最近 200 行，等待超时后终止并回收进程，join stderr 线程 |
| Runner 同步文件对话框 | `src/runner/file_handler.rs` 通过后台任务与 UI 事件回传选择结果 |
| profiler 默认监听所有网卡 | `src/lib.rs` 仅绑定 `127.0.0.1:8585` |
| 大形状文件内联测试与命中逻辑混杂 | 测试移至各自 `drawn_shape/tests.rs`；UI 命中实现位于 `interaction/drawn_shape/hit.rs` |
