//! 文案目录分域：`#[error(...)]` / `Display` 里的**错误短句片段**
//!
//! 这一域不按目录划分，而是按**形态**划分：thiserror 属性宏与手写 `Display`
//! 拿不到「整句」，只能拆成「可译前缀 + 字段」两段式：
//!
//! ```ignore
//! #[error("{}: {}", lumino_core::i18n::tr("IO 错误"), .0)]
//! Io(#[from] std::io::Error),
//! ```
//!
//! 因此这里登记的是**前缀短句**（如 `"IO 错误"`、`"连接失败"`），
//! 英文侧同样只写前缀，由调用点的格式串补分隔符与字段。
//!
//! ⚠️ **不要**把「会被当作数据比较」的错误文案搬进来：例如
//! `ExportError::Aborted` 的 `"已中止"` 在 `ui/.../audio_export.rs` 里被
//! `error == "已中止"` 直接比较，一旦本地化比较就会失效——这类保持原文不动。
//!
//! 已覆盖（13 个源码文件、59 条本域 key）：
//! `tools/export`、`tools/export/video`、`network/cloud`、`network/collaboration`、
//! `audio/midi-loader`、`audio/midiplayer/texture_waterfall`（cache/config/scheduler）、
//! `editor/gfx`（context/global_bucket/device_check）、`editor/ui-core`、`src/runner`。

/// 查本域英文译文；未登记返回 `None`。
pub fn lookup(zh: &str) -> Option<&'static str> {
    match zh {
        // `crates/audio/midi-model/src/error.rs`（LoaderError）
        "IO 错误" => Some("I/O error"),
        "MIDI 解析错误" => Some("MIDI parse error"),
        "缓存错误" => Some("Cache error"),
        "序列化错误" => Some("Serialization error"),
        "压缩错误" => Some("Compression error"),
        "文件格式错误" => Some("Invalid file format"),
        "无效参数" => Some("Invalid argument"),

        // `crates/core/core/src/error.rs`（CoreError）复用上面 7 条，无新增 key。

        // `crates/tools/export/src/error.rs`（ExportError）
        "MIDI 写入错误" => Some("MIDI write error"),
        "音频写入错误" => Some("Audio write error"),
        "无效的导出数据" => Some("Invalid export data"),
        "无效的文件路径" => Some("Invalid file path"),
        "编码错误" => Some("Encoding error"),

        // `crates/tools/export/src/video/error.rs`（VideoExportError）
        "ffmpeg 退出码" => Some("ffmpeg exit code"),
        "ffmpeg 写入失败" => Some("ffmpeg write failed"),
        "未找到 ffmpeg 可执行文件" => Some("ffmpeg executable not found"),
        "ffmpeg 管道建立失败" => Some("Failed to set up ffmpeg pipes"),
        "帧数据尺寸不匹配" => Some("Frame data size mismatch"),

        // `crates/network/cloud/src/error.rs`（CloudError）
        "连接失败" => Some("Connection failed"),
        "认证失败" => Some("Authentication failed"),
        "配置错误" => Some("Configuration error"),
        "加密错误" => Some("Encryption error"),
        "协议错误" => Some("Protocol error"),
        "未连接" => Some("Not connected"),
        // `"操作失败"` 已在 `runner_host` 域登记（Operation failed），此处不重复。

        // `crates/network/collaboration/src/error.rs`（CollaborationError）
        "HTTP 请求失败" => Some("HTTP request failed"),
        "WebSocket 错误" => Some("WebSocket error"),
        "WebSocket 协议错误" => Some("WebSocket protocol error"),
        "JSON 错误" => Some("JSON error"),
        "连接超时" => Some("Connection timed out"),
        "房间未找到" => Some("Room not found"),
        "未连接到服务器" => Some("Not connected to server"),

        // `crates/audio/midi-loader/src/archive.rs`（ArchiveError）
        "不支持的压缩格式" => Some("Unsupported compression format"),
        "文件不是有效的压缩包" => Some("File is not a valid archive"),
        "解压失败" => Some("Extraction failed"),
        "压缩包中未找到条目" => Some("Entry not found in archive"),
        "压缩包条目名不安全（疑似路径穿越攻击），已拒绝" => {
            Some("Unsafe archive entry name (possible path traversal attack), rejected")
        }
        "解压库错误" => Some("Extraction library error"),

        // `crates/audio/midiplayer/src/texture_waterfall/cache/core.rs`（WaterfallCacheError）
        "magic 不匹配" => Some("Magic mismatch"),
        "版本不匹配" => Some("Version mismatch"),
        "元数据序列化/反序列化失败" => {
            Some("Metadata serialization/deserialization failed")
        }
        "像素压缩/解压失败" => Some("Pixel compression/decompression failed"),
        "规格不匹配（缓存失效）" => Some("Specification mismatch (cache invalidated)"),

        // `crates/audio/midiplayer/src/texture_waterfall/config.rs`（WaterfallConfigError）
        "组内小节数必须在 1..=16 之间，当前为" => {
            Some("Measures per group must be within 1..=16, currently")
        }
        "贴图宽度必须在 480..=7680 之间，当前为" => {
            Some("Tile width must be within 480..=7680, currently")
        }
        "冷静期必须在 3..=60 秒之间，当前为" => {
            Some("Cooldown must be within 3..=60 seconds, currently")
        }

        // `crates/audio/midiplayer/src/texture_waterfall/scheduler.rs`（WaterfallGenerateError）
        "缓存 IO 错误" => Some("Cache I/O error"),

        // `crates/editor/gfx/src/context.rs`（ContextError）
        "创建 surface 失败" => Some("Failed to create surface"),
        "创建 adapter 失败" => Some("Failed to create adapter"),
        "获取 preferred format 失败" => Some("Failed to get preferred format"),
        "请求 device 失败" => Some("Failed to request device"),
        "获取帧失败" => Some("Failed to acquire frame"),
        "GPU 尚未初始化" => Some("GPU not initialized"),

        // `crates/editor/gfx/src/global_bucket.rs`（GlobalBucketError）
        "音符数量超出 u32 索引范围" => Some("Note count exceeds u32 index range"),
        "直方图回读 map 失败" => Some("Histogram readback map failed"),
        "直方图回读超时" => Some("Histogram readback timed out"),
        "回读数据长度异常" => Some("Readback data length invalid"),
        "cull 内部资源缺失" => Some("Missing internal cull resource"),

        // `crates/editor/gfx/src/device_check/report.rs`（GpuCheckFailure 的 Display）
        "未找到适配器" => Some("No adapter found"),
        "设备请求失败" => Some("Device request failed"),
        "渲染执行失败" => Some("Rendering failed"),
        "检测超时" => Some("Check timed out"),

        // `crates/editor/ui-core/src/resources/icon.rs`（IconError）
        "图标不在缓存中" => Some("Icon not in cache"),
        "SVG 解析错误" => Some("SVG parse error"),
        "无法创建 pixmap" => Some("Failed to create pixmap"),
        "获取缓存锁失败" => Some("Failed to acquire cache lock"),

        // `src/runner/inner.rs`（InitError）
        "存储初始化失败" => Some("Storage initialization failed"),
        "窗口初始化失败" => Some("Window initialization failed"),
        "云存储初始化失败" => Some("Cloud storage initialization failed"),

        _ => None,
    }
}
