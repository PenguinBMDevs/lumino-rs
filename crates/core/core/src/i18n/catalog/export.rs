//! 文案目录分域：crates/tools/{export,extras}/**（导出管线与扩展工具）
//!
//! 维护约定见 `catalog.rs` 顶部：一行一条 `"中文原文" => Some("English"),`，
//! 命名占位符 `{name}` 两种语言必须完全一致（由 `catalog.rs` 的测试强制）。

/// 查本域英文译文；未登记返回 `None`。
pub fn lookup(zh: &str) -> Option<&'static str> {
    match zh {
        " | {s}× 实时" => Some(" | {s}× realtime"),
        "不支持的 MIDI 源格式: {ext}" => Some("Unsupported MIDI source format: {ext}"),
        "不支持的音色库格式 {path}：仅支持 .sf2/.sfz" => {
            Some("Unsupported soundfont format {path}: only .sf2/.sfz are supported")
        }
        "当前 LMPJ 未包含原始 MIDI 数据，且原始文件不存在，无法导出标准 MIDI" => {
            Some(
                "The current LMPJ embeds no original MIDI data and the original file is missing, so a standard MIDI file cannot be exported",
            )
        }
        "反序列化失败: {e}" => Some("Deserialization failed: {e}"),
        "解析 MIDI 失败: {e}" => Some("Failed to parse MIDI: {e}"),
        "解析 LMPJ 失败: {e}" => Some("Failed to parse LMPJ: {e}"),
        "解压失败: {e}" => Some("Decompression failed: {e}"),
        "进度: {pct}% | 预载音色库 {done}/{total} | 耗时 {elapsed}s" => {
            Some("Progress: {pct}% | Preloading soundfonts {done}/{total} | Elapsed: {elapsed}s")
        }
        "进度: {pct}% | 帧 {done}/{total}{speed_text} | 耗时 {elapsed}s" => {
            Some("Progress: {pct}% | Frames {done}/{total}{speed_text} | Elapsed: {elapsed}s")
        }
        "进度: {pct}% | 事件: {events} | 音符: {notes}{speed_text} | 耗时: {elapsed}s" => {
            Some(
                "Progress: {pct}% | Events: {events} | Notes: {notes}{speed_text} | Elapsed: {elapsed}s",
            )
        }
        "输出路径不是合法 UTF-8: {path}" => Some("Output path is not valid UTF-8: {path}"),
        "未指定音色库文件，请先在 音色库(SF2) 中选择 .sf2 文件" => {
            Some("No soundfont selected. Choose a .sf2 file under Soundfont (SF2) first")
        }
        "未指定音色库文件，请在导出面板选择 .sf2" => {
            Some("No soundfont selected. Choose a .sf2 file in the export panel")
        }
        "无法创建 WAV 文件 {path}: {e}" => Some("Failed to create WAV file {path}: {e}"),
        "无法启动 ffmpeg: {e}" => Some("Failed to start ffmpeg: {e}"),
        "无法启动 stderr 排空线程: {e}" => {
            Some("Failed to start the stderr drain thread: {e}")
        }
        "无法获取 ffmpeg stderr" => Some("Failed to capture ffmpeg stderr"),
        "无法获取 ffmpeg stdin" => Some("Failed to capture ffmpeg stdin"),
        "写入错误: {e}" => Some("Write error: {e}"),
        "序列化失败: {e}" => Some("Serialization failed: {e}"),
        "压缩失败: {e}" => Some("Compression failed: {e}"),
        "ffmpeg 编码失败:\n{stderr_text}" => Some("ffmpeg encoding failed:\n{stderr_text}"),
        "ffmpeg 管道写入错误: {e}" => Some("ffmpeg pipe write error: {e}"),
        "ffmpeg 管道已关闭" => Some("The ffmpeg pipe is closed"),
        "ffmpeg 进程等待失败: {e}" => Some("Failed waiting for the ffmpeg process: {e}"),
        "ffmpeg 进程退出码: {code}" => Some("ffmpeg exit code: {code}"),
        "ffmpeg 退出码 {code}" => Some("ffmpeg exited with code {code}"),
        "ffmpeg 未知错误退出" => Some("ffmpeg exited with an unknown error"),
        "ffmpeg stderr 未 piped" => Some("ffmpeg stderr is not piped"),
        "ffmpeg stdin 未 piped" => Some("ffmpeg stdin is not piped"),
        "FLAC 不支持超过 384kHz 的采样率" => {
            Some("FLAC does not support sample rates above 384 kHz")
        }
        "MP3 不支持超过 48kHz 的采样率" => {
            Some("MP3 does not support sample rates above 48 kHz")
        }
        "MP3 不支持超过 320kbps 的比特率" => {
            Some("MP3 does not support bitrates above 320 kbps")
        }
        "Vorbis 不支持超过 48kHz 的采样率" => {
            Some("Vorbis does not support sample rates above 48 kHz")
        }
        "Vorbis 不支持超过 480kbps 的比特率" => {
            Some("Vorbis does not support bitrates above 480 kbps")
        }
        "PCM 不需要 ffmpeg 编码" => Some("PCM does not require ffmpeg encoding"),
        "需要 ffmpeg 来编码 {ext} 格式，但未找到 ffmpeg" => {
            Some("ffmpeg is required to encode {ext}, but ffmpeg was not found")
        }
        "WAV 写入错误: {e}" => Some("WAV write error: {e}"),
        "WAV 写入器初始化失败: {e}" => Some("Failed to initialize the WAV writer: {e}"),
        "WAV 写入器已关闭" => Some("The WAV writer is closed"),
        "WAV finalize 失败: {e}" => Some("WAV finalize failed: {e}"),
        "write_all 失败: {e}" => Some("write_all failed: {e}"),
        "stdin_writer 已关闭，无法写入帧" => {
            Some("stdin_writer is closed; cannot write frames")
        }
        "GPU 初始化中..." => Some("Initializing GPU..."),
        "GPU 初始化失败: {e}" => Some("GPU initialization failed: {e}"),
        "GPU 初始化失败（可能无可用 Vulkan/Metal 适配器）: {e}" => {
            Some("GPU initialization failed (no usable Vulkan/Metal adapter): {e}")
        }
        "GPU 加载音色库..." => Some("Loading soundfont on GPU..."),
        "GPU 音色库加载失败 {sf_path}: {e}" => {
            Some("Failed to load soundfont on GPU {sf_path}: {e}")
        }
        "GPU 序列化 MIDI（内存）..." => Some("GPU serializing MIDI (in memory)..."),
        "GPU 渲染中..." => Some("GPU rendering..."),
        "GPU 渲染中（可能耗时，黑 MIDI 请耐心）..." => {
            Some("GPU rendering (this may take a while; heavy MIDI needs patience)...")
        }
        "GPU 渲染失败: {other}" => Some("GPU rendering failed: {other}"),
        "GPU 写入输出..." => Some("GPU writing output..."),
        "GPU 写入 {pct}%" => Some("GPU writing {pct}%"),
        "GPU 完成" => Some("GPU finished"),
        "音色库文件不存在: {path}" => Some("Soundfont file not found: {path}"),
        "音色库文件不存在: {path}，请检查路径或重新选择" => {
            Some("Soundfont file not found: {path}. Check the path or select it again")
        }
        "音色库不是合法的 SF2 {path}：头应为 RIFF，实际 {header}" => {
            Some("{path} is not a valid SF2 file: expected a RIFF header, got {header}")
        }
        "音色库不是合法的 SF2 文件 {path}：头应为 RIFF，实际 {header}" => {
            Some("{path} is not a valid SF2 file: expected a RIFF header, got {header}")
        }
        "音色库 {sf_path}: {e}" => Some("Soundfont {sf_path}: {e}"),
        "MIDI 文件不存在: {midi_path}" => Some("MIDI file not found: {midi_path}"),
        "MIDI 文档中没有可渲染的事件" => {
            Some("The MIDI document contains no renderable events")
        }
        "MIDI 文档中没有可渲染的事件（0 notes），请检查 MIDI 是否已加载" => {
            Some(
                "The MIDI document contains no renderable events (0 notes); check whether the MIDI is loaded",
            )
        }
        _ => None,
    }
}
