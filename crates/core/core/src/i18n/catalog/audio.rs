//! 文案目录分域：crates/audio/**（MIDI / 音频 / 播放）
//!
//! 维护约定见 `catalog.rs` 顶部：一行一条 `"中文原文" => Some("English"),`，
//! 命名占位符 `{name}` 两种语言必须完全一致（由 `catalog.rs` 的测试强制）。
//!
//! 说明：本域只收录**会回显给用户的错误串、加载进度与状态文案**；
//! `tracing::*!` 日志、`debug_assert!`、内部二进制格式诊断（clipboard / domino /
//! 贴图缓存完整性校验）一律保持中文原文不动。

/// 查本域英文译文；未登记返回 `None`。
pub fn lookup(zh: &str) -> Option<&'static str> {
    match zh {
        // ── midi-io：后端初始化 / 设备 / 输出连接 ──
        "无可用音频输出设备" => Some("No audio output device available"),
        "读取默认输出配置失败: {e}" => {
            Some("Failed to read the default output configuration: {e}")
        }
        "创建输出流失败: {e}" => Some("Failed to create output stream: {e}"),
        "暂停输出流失败: {e}" => Some("Failed to pause the output stream: {e}"),
        "探测音频格式失败: {e}" => Some("Failed to probe the audio format: {e}"),
        "音频文件无可用轨道" => Some("The audio file has no usable track"),
        "创建解码器失败: {e}" => Some("Failed to create decoder: {e}"),
        "读取音频包失败: {e}" => Some("Failed to read audio packet: {e}"),
        "解码音频包失败: {e}" => Some("Failed to decode audio packet: {e}"),
        "音频解码结果为空" => Some("Audio decoding produced no data"),
        "当前后端不支持音频流恢复" => {
            Some("The current backend does not support audio stream recovery")
        }
        "MIDI 输入未初始化" => Some("MIDI input not initialized"),
        "MIDI 输出未初始化" => Some("MIDI output not initialized"),
        "MIDI 文件为空" => Some("The MIDI file is empty"),
        "不是普通文件，无法按 MIDI 加载" => {
            Some("Not a regular file; cannot load it as MIDI")
        }
        "准备解析 ({file_size_mb} MB)，开始映射..." => {
            Some("Preparing to parse ({file_size_mb} MB), mapping...")
        }
        "正在映射文件... (内存: {map_rss} MB)" => {
            Some("Mapping file... (memory: {map_rss} MB)")
        }
        "该压缩格式（RAR/7Z/LZH）在 Android 上暂不支持：unarc-rs 无 android 后端" => {
            Some(
                "This archive format (RAR/7Z/LZH) is not supported on Android yet: unarc-rs has no Android backend",
            )
        }
        "KDMAPI 不可用（IsKDMAPIAvailable 返回 false）。请在 OmniMIDI 设置中启用 KDMAPI" => {
            Some(
                "KDMAPI is unavailable (IsKDMAPIAvailable returned false). Please enable KDMAPI in the OmniMIDI settings.",
            )
        }
        "KDMAPI 关键符号缺失: {list}。可能为 OmniMIDI v2（已移除部分旧版符号）或该 DLL 非 KDMAPI 构建；请升级或重装 OmniMIDI 后重试" => {
            Some(
                "KDMAPI is missing required symbols: {list}. This may be OmniMIDI v2 (some legacy symbols were removed) or a non-KDMAPI build of the DLL; please update or reinstall OmniMIDI and try again",
            )
        }
        "初始化 KDMAPI 流失败（InitializeKDMAPIStream 返回 0）。若使用 OmniMIDI v2，请检查其 settings.json 的 renderer/合成器配置（指向未编译的模块会导致失败）与音色库选择；确认驱动状态后重试" => {
            Some(
                "Failed to initialize the KDMAPI stream (InitializeKDMAPIStream returned 0). If you are using OmniMIDI v2, check the renderer/synth configuration in its settings.json (pointing to an uncompiled module will fail) and the selected soundfont, then verify the driver status and retry",
            )
        }
        "KDMAPI 不支持 MIDI 输入" => Some("KDMAPI does not support MIDI input"),
        "OmniMIDI.dll 未找到。已尝试路径: {paths}。请确认 OmniMIDI 已安装（https://github.com/KeppySoftware/OmniMIDI）" => {
            Some(
                "OmniMIDI.dll not found. Tried paths: {paths}. Please make sure OmniMIDI is installed (https://github.com/KeppySoftware/OmniMIDI)",
            )
        }
        "LGS (GPU) 初始化失败: {e}" => Some("LGS (GPU) initialization failed: {e}"),
        "LGS (GPU) 音色库加载失败: {e}" => {
            Some("LGS (GPU) failed to load the soundfont: {e}")
        }
        "LGS (GPU) 音频流启动失败: {e}" => {
            Some("LGS (GPU) failed to start the audio stream: {e}")
        }
        "LGS (GPU): 复位请求发送失败（渲染线程已停止）" => {
            Some("LGS (GPU): failed to send the reset request (render thread has stopped)")
        }
        "重建 GPU 合成引擎失败: {e}" => Some("Failed to rebuild the GPU synth engine: {e}"),
        "重启 GPU 音频流失败: {e}" => Some("Failed to restart the GPU audio stream: {e}"),
        "音频设备默认配置失败: {e}" => {
            Some("Failed to get the audio device default configuration: {e}")
        }
        "重建合成管线失败: {e}" => Some("Failed to rebuild the synth pipeline: {e}"),
        "xsynth 不支持的消息类型: 0x{code}" => {
            Some("Unsupported message type for xsynth: 0x{code}")
        }

        // ── midi-loader：加载 / 解压 / 扫描 ──
        "无法获取文件扩展名" => Some("Cannot get the file extension"),
        "文件是压缩包格式，请先解压后再加载。" => {
            Some("This file is an archive. Please extract it before loading.")
        }
        "本文件不支持加载，请检查文件格式！支持的格式：.mid / .midi / .lmpj" => {
            Some(
                "This file is not supported. Please check the file format! Supported formats: .mid / .midi / .lmpj",
            )
        }
        "正在加载 Lumino 工程文件" => Some("Loading Lumino project file"),
        "解析 Lumino 工程文件" => Some("Parsing Lumino project file"),
        "Lumino 工程文件加载完成" => Some("Lumino project file loaded"),
        "正在准备加载文件 (内存: {initial_rss} MB)" => {
            Some("Preparing to load file (memory: {initial_rss} MB)")
        }
        "正在读取文件... (内存: {read_rss} MB)" => {
            Some("Reading file... (memory: {read_rss} MB)")
        }
        "文件读取完成 ({file_size_mb} MB)，开始解析..." => {
            Some("File read complete ({file_size_mb} MB), parsing...")
        }
        "正在提取音符并构建缓存... (内存: {inner_rss} MB)" => {
            Some("Extracting notes and building cache... (memory: {inner_rss} MB)")
        }
        "正在解析 MIDI 数据... (内存: {parse_rss} MB)" => {
            Some("Parsing MIDI data... (memory: {parse_rss} MB)")
        }
        "MIDI 加载完成 ({total_notes} 音符, 内存: {rss_mb} MB)" => {
            Some("MIDI loaded ({total_notes} notes, memory: {rss_mb} MB)")
        }
        "MIDI 加载完成 (内存: {final_rss} MB)" => {
            Some("MIDI loaded (memory: {final_rss} MB)")
        }
        "加载线程 panic: {e}" => Some("Loading thread panicked: {e}"),
        "解析 MIDI 数据失败: {e}" => Some("Failed to parse MIDI data: {e}"),
        "扫描 MIDI 文件失败: {e}" => Some("Failed to scan the MIDI file: {e}"),
        "MmapSmf 解析失败: {e}" => Some("Failed to parse MmapSmf: {e}"),
        "未知" => Some("Unknown"),
        "unarc-rs 打开失败: {e}" => Some("unarc-rs failed to open: {e}"),
        "unarc-rs 读取条目失败: {e}" => Some("unarc-rs failed to read entry: {e}"),
        "unarc-rs 读取数据失败: {e}" => Some("unarc-rs failed to read data: {e}"),
        "zip 打开失败: {e}" => Some("zip failed to open: {e}"),
        "zip 读取条目失败: {e}" => Some("zip failed to read entry: {e}"),
        "zip 读取数据失败: {e}" => Some("zip failed to read data: {e}"),
        "GZ 解压失败: {e}" => Some("GZ decompression failed: {e}"),
        "XZ 解压失败: {e}" => Some("XZ decompression failed: {e}"),
        "iso9660 读取条目失败: {e}" => Some("iso9660 failed to read entry: {e}"),
        "iso9660 读取文件失败: {e}" => Some("iso9660 failed to read file: {e}"),
        "iso9660 查找失败: {e}" => Some("iso9660 lookup failed: {e}"),
        "tar 读取条目失败: {e}" => Some("tar failed to read entry: {e}"),
        "tar 条目解析失败: {e}" => Some("tar failed to parse entry: {e}"),
        "tar 路径解析失败: {e}" => Some("tar failed to parse path: {e}"),
        "tar 读取数据失败: {e}" => Some("tar failed to read data: {e}"),
        "tar 解压失败: {e}" => Some("tar extraction failed: {e}"),
        "tar.gz 解压失败: {e}" => Some("tar.gz extraction failed: {e}"),
        "tar.xz 解压失败: {e}" => Some("tar.xz extraction failed: {e}"),

        // ── midi-model：文档构建 ──
        "无法解析 MIDI 文件头" => Some("Cannot parse the MIDI file header"),
        "提取音符失败: {e}" => Some("Failed to extract notes: {e}"),

        // ── midiplayer：贴图瀑布流生成进度与状态 ──
        "正在后台生成贴图瀑布流\u{2026}" => {
            Some("Generating texture waterfall in the background\u{2026}")
        }
        "贴图瀑布流资源已释放" => Some("Texture waterfall resources released"),
        "贴图瀑布流流式生成+上传完成" => {
            Some("Texture waterfall streaming generation and upload complete")
        }
        "音轨组 {track_group} 脏区域临时覆层已生成" => {
            Some("Temporary dirty-region overlay generated for track group {track_group}")
        }
        "贴图瀑布流：无内容需生成" => Some("Texture waterfall: nothing to generate"),
        "贴图瀑布流生成完成" => Some("Texture waterfall generation complete"),
        "贴图瀑布流流式生成完成" => {
            Some("Texture waterfall streaming generation complete")
        }
        "贴图瀑布流 time_group {done}/{total}" => {
            Some("Texture waterfall time_group {done}/{total}")
        }
        "贴图瀑布流 {done}/{total}" => Some("Texture waterfall {done}/{total}"),

        // ── gpu-synth：配置与 GPU 缓冲校验 ──
        "GPU 引擎仅支持立体声（ChannelMode::Stereo）；单声道请在写出前降混" => {
            Some(
                "The GPU engine only supports stereo (ChannelMode::Stereo); downmix mono before writing output",
            )
        }
        "buffer size 为 0（bytes={bytes}）" => Some("buffer size is 0 (bytes={bytes})"),

        // ── midi-loader：MidiInfo 显示（列表元信息）──
        "MIDI文件: {path} | 音轨数: {track_count} | 音符事件数: {total_notes} | 时长: {duration_ticks} ticks | 分辨率: {division}" => {
            Some(
                "MIDI file: {path} | Tracks: {track_count} | Note events: {total_notes} | Length: {duration_ticks} ticks | Resolution: {division}",
            )
        }

        // ── note-core：编辑模式显示名 ──
        "力度" => Some("Velocity"),
        "速度" => Some("Tempo"),
        _ => None,
    }
}
