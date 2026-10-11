//! 文案目录分域：crates/editor/{gfx,dialog}/**（渲染与原生对话框宿主）
//!
//! 维护约定见 `catalog.rs` 顶部：一行一条 `"中文原文" => Some("English"),`，
//! 命名占位符 `{name}` 两种语言必须完全一致（由 `catalog.rs` 的测试强制）。

/// 查本域英文译文；未登记返回 `None`。
pub fn lookup(zh: &str) -> Option<&'static str> {
    match zh {
        // ── 对话框窗口标题（`crates/editor/dialog/src/window.rs`）──
        "自定义贴合" => Some("Custom Precision"),
        "多人协作" => Some("Collaboration"),
        "加载大文件" => Some("Load Large File"),
        "工程设置" => Some("Project Settings"),
        "设置" => Some("Settings"),
        "变速" => Some("Speed Change"),
        "批量编辑" => Some("Batch Edit"),
        "音频导出" => Some("Audio Export"),
        "视频导出" => Some("Video Export"),
        "内存占用详情" => Some("Memory Usage Details"),
        "找回删除音轨" => Some("Recover Deleted Tracks"),
        "连接云存储" => Some("Connect to Cloud Storage"),
        "云存储文件" => Some("Cloud Files"),
        "云存储提醒" => Some("Cloud Storage Notice"),
        "画刷绘制行为" => Some("Brush Drawing Behavior"),
        "音符画设置" => Some("Note Drawing Settings"),
        // ── 视频导出进度初始文案（`crates/editor/dialog/src/window/initialize.rs`）──
        "正在初始化..." => Some("Initializing..."),
        // ── GPU 兼容性检测（`crates/editor/gfx/src/device_check/**`）──
        // 检测结论会渲染到启动警告窗、设置页「兼容性」结果与「复制诊断信息」，
        // 因此内部失败原因按用户可见文案登记。
        "检测线程意外退出" => Some("GPU check thread exited unexpectedly"),
        "检测线程创建失败: {e}" => Some("Failed to create the check thread: {e}"),
        "调试开关 LUMINO_FORCE_DEVICE_CHECK_FAIL 强制失败" => {
            Some("Debug switch LUMINO_FORCE_DEVICE_CHECK_FAIL forces a failure")
        }
        "渲染期间出现 wgpu 错误: {errors}" => Some("wgpu error during rendering: {errors}"),
        "回读映射失败: {e}" => Some("Readback mapping failed: {e}"),
        "离屏画面为空白（{buffer_size} 字节均为清屏色，未画出卷帘）" => Some(
            "Offscreen frame is blank ({buffer_size} bytes are all clear color, piano roll not drawn)",
        ),
        "离屏回读超时（2 秒）" => Some("Offscreen readback timed out (2 s)"),
        "{name} [{backend}] {device_type} · 驱动 {driver} {driver_info}" => {
            Some("{name} [{backend}] {device_type} · Driver {driver} {driver_info}")
        }
        "{backend} 兼容性检测通过" => Some("{backend} compatibility check passed"),
        "{backend} 兼容性检测失败: {failure}" => {
            Some("{backend} compatibility check failed: {failure}")
        }
        "{backend} 兼容性检测失败" => Some("{backend} compatibility check failed"),
        "（耗时 {ms}ms）" => Some("({ms} ms)"),
        "\n指纹: {fingerprint}" => Some("\nFingerprint: {fingerprint}"),
        "\n其他可用后端（回退参考）:" => {
            Some("\nOther available backends (fallback reference):")
        }
        _ => None,
    }
}
