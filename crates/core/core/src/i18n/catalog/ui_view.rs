//! 文案目录分域：`crates/editor/ui/src/view/**`（各对话框正文）
//!
//! 维护约定见 `catalog.rs` 顶部：一行一条 `"中文原文" => Some("English"),`，
//! 命名占位符 `{name}` 两种语言必须完全一致（由 `catalog.rs` 的测试强制）。

/// 查本域英文译文；未登记返回 `None`。
pub fn lookup(zh: &str) -> Option<&'static str> {
    match zh {
        "帮助" => Some("Help"),
        "搜索帮助..." => Some("Search help..."),
        "没有匹配的话题" => Some("No matching topics"),
        "全部重现" => Some("Show all again"),
        "查看帮助" => Some("View help"),
        "知道了" => Some("Got it"),
        "曲线" => Some("Curve"),
        "折线" => Some("Polyline"),
        // ── 云存储连接面板 / 断连提醒（`view/cloud_dialog.rs`）──
        "Lumino 无权且不会收集您的个人信息，所有信息将会存储在本地" => {
            Some(
                "Lumino neither accesses nor collects your personal information; everything stays on your device",
            )
        }
        "云存储连接异常" => Some("Cloud storage connection error"),
        "云存储提醒" => Some("Cloud Storage Notice"),
        "协议" => Some("Protocol"),
        "名称（可选）" => Some("Name (optional)"),
        "例如：我的 NAS" => Some("e.g. My NAS"),
        "服务器地址" => Some("Server address"),
        "端口（可选）" => Some("Port (optional)"),
        "留空使用默认端口" => Some("Leave blank for the default port"),
        "用户名" => Some("Username"),
        "密码（可选）" => Some("Password (optional)"),
        "密码" => Some("Password"),
        "连接失败：{e}" => Some("Connection failed: {e}"),
        "连接中..." => Some("Connecting..."),
        "连接" => Some("Connect"),
        "取消" => Some("Cancel"),
        "连接云存储" => Some("Connect to Cloud Storage"),
        // ── 音频导出对话框（`view/audio_export_dialog*`）──
        "音频导出" => Some("Audio Export"),
        "音频设置" => Some("Audio Settings"),
        "渲染后端:" => Some("Render backend:"),
        "CPU 兼容性最好；GPU 需 Vulkan/Metal，无适配器时自动回退到 CPU，速度提升 2-5 倍" => {
            Some(
                "CPU is the most compatible; GPU needs Vulkan/Metal and falls back to CPU when no adapter is available, 2-5x faster",
            )
        }
        "输出格式:" => Some("Output format:"),
        "比特率 (kbps):" => Some("Bitrate (kbps):"),
        "仅 MP3/Ogg 有效；WAV/FLAC 忽略此值" => {
            Some("Only applies to MP3/Ogg; ignored for WAV/FLAC")
        }
        "采样率 (Hz):" => Some("Sample rate (Hz):"),
        "MP3 最高 48000 Hz，FLAC 最高 384000 Hz，超出会自动报错" => Some(
            "MP3 supports up to 48000 Hz and FLAC up to 384000 Hz; higher values raise an error",
        ),
        "通道数:" => Some("Channels:"),
        "通道多线程 (CPU):" => Some("Channel multithreading (CPU):"),
        "按键多线程 (CPU):" => Some("Key multithreading (CPU):"),
        "插值算法:" => Some("Interpolation:"),
        "无插值有混叠，线性为默认（GPU 固定线性，与 CPU 线性一致）" => {
            Some(
                "No interpolation causes aliasing; linear is the default (GPU is fixed to linear, matching CPU linear)",
            )
        }
        "线性包络（CPU 衰减/释音用线性，GPU 默认线性，关闭则全指数）" => {
            Some(
                "Linear envelope (CPU uses linear decay/release, GPU defaults to linear; off = fully exponential)",
            )
        }
        "启用限幅器（GPU 引擎始终限幅，此选项对 GPU 无效）" => {
            Some("Enable limiter (GPU engine always limits; no effect on GPU)")
        }
        "启用限幅器（大复音防削波，会轻微影响响度，建议开启）" => Some(
            "Enable limiter (prevents clipping with many voices; slightly affects loudness, recommended)",
        ),
        "禁用淡出（GPU 引擎固定 1ms 淡出，此选项对 GPU 无效）" => {
            Some("Disable fade-out (GPU engine uses a fixed 1 ms fade; no effect on GPU)")
        }
        "禁用淡出（voice 被抢占时硬切，会产生咔哒声，不建议开启）" => {
            Some(
                "Disable fade-out (hard-cuts when a voice is stolen, causing clicks; not recommended)",
            )
        }
        "每键复音 (0=不限):" => Some("Polyphony per key (0 = unlimited):"),
        "每个键（通道+音高）最大同时发声数；0=不限（不推荐）。建议保持 32：实测同素材全曲 32 → 165s、0 → 59min（声部无限堆积，耗时与内存暴涨）。CPU 按 voice 计、GPU 按音符组计。" => {
            Some(
                "Maximum simultaneous voices per key (channel + pitch); 0 = unlimited (not recommended). Keep 32: on the same material a full song took 32 → 165s vs 0 → 59min (voices pile up endlessly, time and memory explode). CPU counts voices; GPU counts note groups.",
            )
        }
        "⚠ 已设为不限：密集素材下声部会无限堆积，渲染时间与内存会显著恶化——除非明确需要，建议改回 32。" => {
            Some(
                "⚠ Unlimited: voices pile up endlessly with dense material and render time and memory degrade sharply — set it back to 32 unless you really need this.",
            )
        }
        "继续" => Some("Resume"),
        "暂停" => Some("Pause"),
        "中止" => Some("Abort"),
        "重置" => Some("Reset"),
        "关闭" => Some("Close"),
        "已暂停" => Some("Paused"),
        "导出完成" => Some("Export complete"),
        "已中止" => Some("Aborted"),
        "导出失败: {err}" => Some("Export failed: {err}"),
        "导出" => Some("Export"),
        "事件过滤" => Some("Event Filtering"),
        "忽略音色变化事件（Program Change）" => Some("Ignore program change events"),
        "勾选后所有音色切换事件被丢弃，所有音符用默认音色渲染" => Some(
            "When checked, all program change events are discarded and every note uses the default instrument",
        ),
        "音符力度过滤（仅保留指定力度范围内的音符）" => {
            Some("Note velocity filtering (keep only notes within the specified velocity range)")
        }
        "启用力度过滤（超出范围的音符将被静音丢弃）" => {
            Some("Enable velocity filtering (out-of-range notes are silenced and dropped)")
        }
        "力度范围 (0-127):" => Some("Velocity range (0-127):"),
        "音符键位过滤（仅保留指定键位范围内的音符，0=C-1, 60=C4, 127=G9）" => {
            Some(
                "Note key filtering (keep only notes within the specified key range; 0=C-1, 60=C4, 127=G9)",
            )
        }
        "启用键位过滤（超出范围的音符将被静音丢弃）" => {
            Some("Enable key filtering (out-of-range notes are silenced and dropped)")
        }
        "键位范围 (0-127):" => Some("Key range (0-127):"),
        "音符结束延迟 (ms):" => Some("Note end delay (ms):"),
        "每个音符 NoteOff 后额外延长该时长再释放，>0 可避免极短音被吞（0=禁用）" => {
            Some(
                "Extend each note by this duration after NoteOff before releasing; >0 avoids swallowing very short notes (0 = disabled)",
            )
        }
        "输出路径" => Some("Output Path"),
        "选择输出路径..." => Some("Choose output path..."),
        "浏览..." => Some("Browse..."),
        "工程名称" => Some("Project Name"),
        "MIDI 路径" => Some("MIDI Path"),
        "音色库 (SF2)" => Some("Soundfont (SF2)"),
        // ── 批量编辑对话框（`view/batch_edit_dialog.rs`）──
        "批量编辑" => Some("Batch Edit"),
        "设置" => Some("Settings"),
        "音符力度:" => Some("Note velocity:"),
        "如: +20" => Some("e.g. +20"),
        "音符长度:" => Some("Note length:"),
        "如: *1.5" => Some("e.g. *1.5"),
        "音符key位置:" => Some("Note key position:"),
        "如: 60" => Some("e.g. 60"),
        "音符tick位置:" => Some("Note tick position:"),
        "如: 1000" => Some("e.g. 1000"),
        "格式" => Some("Format"),
        "支持以下运算符号:" => Some("Supported operators:"),
        "+N  增加 N        -N  减少 N" => Some("+N  add N        -N  subtract N"),
        "*N  乘以 N        /N  除以 N" => Some("*N  multiply by N        /N  divide by N"),
        "无符号数字表示直接设置为该值" => {
            Some("A plain number sets the value directly")
        }
        "确定" => Some("OK"),
        // ── 画刷绘制行为对话框（`view/brush_settings_dialog.rs`）──
        "第 {n} 层" => Some("Layer {n}"),
        "（无可用音轨）" => Some("(No tracks available)"),
        "默认（自动分配）" => Some("Default (auto-assign)"),
        "音轨 {id}" => Some("Track {id}"),
        "无可用音轨，请先在工程中创建普通音轨。" => {
            Some("No tracks available. Create a regular track in the project first.")
        }
        "画刷绘制行为设置" => Some("Brush Behavior Settings"),
        "保存" => Some("Save"),
        // ── 云文件浏览面板（`view/cloud_browser.rs`）──
        "在线" => Some("Online"),
        "离线" => Some("Offline"),
        "选择云存储" => Some("Select cloud storage"),
        "粘贴（剪切）" => Some("Paste (cut)"),
        "粘贴" => Some("Paste"),
        "清空剪贴板" => Some("Clear clipboard"),
        "刷新" => Some("Refresh"),
        "断开" => Some("Disconnect"),
        "← 返回" => Some("← Back"),
        "新建文件夹名称" => Some("New folder name"),
        "新建文件夹" => Some("New Folder"),
        "加载中..." => Some("Loading..."),
        "（空目录）" => Some("(Empty folder)"),
        "将上传素材 {name} 到：{path}" => Some("Upload {name} to: {path}"),
        "将保存当前工程到：{path}" => Some("Save the current project to: {path}"),
        "上传中..." => Some("Uploading..."),
        "保存到此处" => Some("Save here"),
        "新名称" => Some("New name"),
        "确认删除？" => Some("Delete this item?"),
        "删除" => Some("Delete"),
        "下载" => Some("Download"),
        "剪切" => Some("Cut"),
        "复制" => Some("Copy"),
        "重命名" => Some("Rename"),
        // ── 多人协作对话框（`view/collaboration_dialog.rs`）──
        "多人协作" => Some("Collaboration"),
        "端口" => Some("Port"),
        "用户" => Some("User"),
        "邀请码（可选）" => Some("Invite code (optional)"),
        "正在连接服务器..." => Some("Connecting to server..."),
        "请稍候" => Some("Please wait"),
        "房间名称" => Some("Room name"),
        "创建房间" => Some("Create Room"),
        "- 或 -" => Some("- or -"),
        "邀请码" => Some("Invite code"),
        "加入房间" => Some("Join Room"),
        "房间: {name}" => Some("Room: {name}"),
        "邀请码: " => Some("Invite code: "),
        "复制邀请码" => Some("Copy invite code"),
        "断开连接" => Some("Disconnect"),
        // ── 加载确认对话框（`view/load_confirm_dialog.rs`）──
        "文件: {name}" => Some("File: {name}"),
        "大小: {size} MB" => Some("Size: {size} MB"),
        "路径: {path}" => Some("Path: {path}"),
        "此文件较大，加载可能需要一些时间。" => {
            Some("This file is large; loading may take a while.")
        }
        "加载" => Some("Load"),
        // ── 内存监控对话框（`view/memory_monitor_dialog.rs`）──
        "内存占用详情" => Some("Memory Usage Details"),
        "分配器追踪总内存" => Some("Total tracked by allocator"),
        "系统 RSS" => Some("System RSS"),
        "GPU 资源" => Some("GPU resources"),
        "按子系统分类" => Some("By subsystem"),
        "注：GPU 资源计数反映应用显式创建的 wgpu Texture/Buffer 大小；驱动层额外开销（swapchain、depth、pipeline cache 等）不纳入此项统计。" => {
            Some(
                "Note: GPU resource counts reflect the size of wgpu Texture/Buffer objects the app explicitly creates; extra driver-level overhead (swapchain, depth, pipeline cache, etc.) is not included.",
            )
        }
        // ── 保存确认对话框（`view/save_confirm_dialog.rs`）──
        "是否保留未保存的更改？" => Some("Keep unsaved changes?"),
        // ── 变速对话框（`view/speed_change_dialog.rs`）──
        "变速" => Some("Speed Change"),
        "倍率(S):" => Some("Factor (S):"),
        "如: 0.5 或 1/3" => Some("e.g. 0.5 or 1/3"),
        "支持小数 (0.5) 或分数 (1/3)" => Some("Supports decimals (0.5) or fractions (1/3)"),
        // ── 工程设置对话框（`view/project_settings_dialog.rs`）──
        "拍号" => Some("Time Signature"),
        // ── 找回删除音轨对话框（`view/recover_track_dialog.rs`）──
        "找回删除音轨" => Some("Recover Deleted Tracks"),
        "选中条目后点击「恢复」可还原至原位置；「永久删除」将销毁缓存文件并释放轨道编号。" => {
            Some(
                "Select an entry and click Restore to put it back in its original position; Permanent Delete destroys the cache file and frees the track number.",
            )
        }
        "文件名" => Some("File name"),
        "删除时间" => Some("Deleted at"),
        "音符总数" => Some("Total notes"),
        "暂无已删除的音轨缓存" => Some("No deleted track caches"),
        "恢复" => Some("Restore"),
        "永久删除" => Some("Permanent Delete"),
        // ── 视频剪辑剪辑带（`view/video_clip/timeline.rs`、`timeline_canvas/draw.rs`）──
        "剪辑带" => Some("Clip Timeline"),
        "时长 {dur}s  位置 {pos}s  缩放 {zoom}x   空格 播放/暂停" => {
            Some("Duration {dur}s  Position {pos}s  Zoom {zoom}x   Space Play/Pause")
        }
        "视频" => Some("Video"),
        "音频" => Some("Audio"),
        // ── 视频导出 · 计数器设置（`view/video_export_dialog/counter_settings.rs`）──
        "计数器设置" => Some("Counter Settings"),
        "文本模板（支持 {nc} {nps} {bpm} 等占位符，\\n 换行）" => {
            Some("Text template (supports {nc} {nps} {bpm} placeholders; \\n for a line break)")
        }
        "恢复默认" => Some("Restore Defaults"),
        "完整模板" => Some("Full Template"),
        "对齐:" => Some("Alignment:"),
        "字号:" => Some("Font size:"),
        "使用千分位分隔符（1,234,567）" => Some("Use thousands separators (1,234,567)"),
        "数字补零（按下方宽度对齐）" => {
            Some("Zero-pad numbers (aligned to the widths below)")
        }
        "路径:" => Some("Path:"),
        "选择 CSV 输出路径..." => Some("Choose CSV output path..."),
        "格式:" => Some("Format:"),
        "每帧统计数据写入 CSV 文件" => Some("Write per-frame statistics to a CSV file"),
        "数字补零宽度" => Some("Zero-padding widths"),
        "BPM 整数" => Some("BPM integer"),
        "BPM 小数" => Some("BPM decimal"),
        "音符数" => Some("Notes"),
        "复音数" => Some("Polyphony"),
        "小节" => Some("Bars"),
        "帧数" => Some("Frames"),
        "恢复默认补零宽度" => Some("Restore default padding widths"),
        // ── 视频导出 · 计数器/数据曲线字体（`counter_settings_font.rs`）──
        "选择 TTF/OTF 字体文件..." => Some("Choose a TTF/OTF font file..."),
        "字体:" => Some("Font:"),
        "内置 5x7 点阵字体，仅支持 ASCII；中文模板请选择系统字体或自定义字体" => {
            Some(
                "Built-in 5x7 bitmap font; ASCII only. For Chinese templates choose a system font or a custom font.",
            )
        }
        // ── 视频导出 · 数据曲线设置（`view/video_export_dialog/data_curve_settings.rs`）──
        "数据曲线设置" => Some("Data Curve Settings"),
        "数据来源:" => Some("Data source:"),
        "数据由 MIDI 统计状态按帧直传，无需外部文件" => {
            Some("Data is streamed per frame from MIDI statistics; no external file needed")
        }
        "曲线参数" => Some("Curve Parameters"),
        "窗口时长(秒)" => Some("Window duration (s)"),
        "缩放平滑" => Some("Zoom smoothing"),
        "折线平滑" => Some("Line smoothing"),
        "纵轴 padding" => Some("Y-axis padding"),
        "折线平滑=0 关闭；越大曲线越平滑但更趋直线" => {
            Some("Line smoothing = 0 disables it; higher values are smoother but flatten the curve")
        }
        "文字与刻度" => Some("Text and Ticks"),
        "X 偏移(px)" => Some("X offset (px)"),
        "Y 偏移(px)" => Some("Y offset (px)"),
        "里程碑放大" => Some("Milestone scale"),
        "缩写位数" => Some("Abbreviation digits"),
        "刻度数字缩写（1,000 → 1K）" => Some("Abbreviate tick numbers (1,000 → 1K)"),
        "显示刻度文字" => Some("Show tick text"),
        "显示水平网格线" => Some("Show horizontal grid lines"),
        "里程碑 = 1,000 / 10,000 / 100,000 等整数次幂刻度，文字放大显示" => Some(
            "Milestones are integer powers of ten (1,000 / 10,000 / 100,000 …) shown with enlarged text",
        ),
        // ── 视频导出 · 数据曲线外观（`view/video_export_dialog/data_curve_settings_style.rs`）──
        "外观" => Some("Appearance"),
        "背景色" => Some("Background color"),
        "折线色" => Some("Line color"),
        "文字色" => Some("Text color"),
        "网格线色" => Some("Grid line color"),
        "折线宽(px)" => Some("Line width (px)"),
        "网格线宽(px)" => Some("Grid line width (px)"),
        "支持 8 位 hex 带透明度，如 #FFFFFF7F" => {
            Some("Supports 8-digit hex with alpha, e.g. #FFFFFF7F")
        }
        "内置 5x7 点阵字体，仅支持 ASCII 数字与字母" => {
            Some("Built-in 5x7 bitmap font; ASCII digits and letters only")
        }
        // ── 视频导出 · 覆盖层状态（`view/video_export_dialog/handlers.rs`）──
        "视频导出中" => Some("Exporting video"),
        "取消导出" => Some("Cancel Export"),
        "正在完成编码..." => Some("Finalizing encoding..."),
        "ffmpeg 正在封装文件，请稍候" => Some("ffmpeg is muxing the file, please wait"),
        "强制完成" => Some("Force Finish"),
        "视频已可用，跳过等待" => Some("Video is ready; skip waiting"),
        "导出完成！" => Some("Export complete!"),
        "总帧数: {n}" => Some("Total frames: {n}"),
        "时长: {d}" => Some("Duration: {d}"),
        "总用时: {t}" => Some("Total time: {t}"),
        "平均速度: {fps} fps" => Some("Average speed: {fps} fps"),
        "倍率: {ratio}x 原速" => Some("Speedup: {ratio}x realtime"),
        "导出失败" => Some("Export failed"),
        // ── 视频导出 · 进度明细（`view/video_export_dialog/helpers.rs`）──
        "帧: {current} / {total}" => Some("Frame: {current} / {total}"),
        "时间: {elapsed} / {total}" => Some("Time: {elapsed} / {total}"),
        "渲染速度: {fps} fps" => Some("Render speed: {fps} fps"),
        "速度: {speed}x 原速" => Some("Speed: {speed}x realtime"),
        "已用: {elapsed} / 剩余: {remaining}" => {
            Some("Elapsed: {elapsed} / Remaining: {remaining}")
        }
        "已用: {elapsed}" => Some("Elapsed: {elapsed}"),
        // ── 视频导出 · 共享布局（`view/video_export_dialog/layout.rs`）──
        "视频导出" => Some("Video Export"),
        "优先使用当前工程的 MIDI 数据" => {
            Some("Use the current project's MIDI data when available")
        }
        "使用指定 MIDI 文件流式读取" => Some("Stream from the specified MIDI file"),
        "MIDI 数据源" => Some("MIDI Data Source"),
        "导出位置" => Some("Export Location"),
        "开始导出" => Some("Start Export"),
        "等待渲染..." => Some("Waiting for render..."),
        // ── 视频导出 · 渲染设置（`view/video_export_dialog/render_settings.rs`）──
        "渲染设置" => Some("Render Settings"),
        "帧率:" => Some("Frame rate:"),
        "渲染格式:" => Some("Render format:"),
        "编码器:" => Some("Encoder:"),
        "加速:" => Some("Acceleration:"),
        "质量:" => Some("Quality:"),
        "渲染风格:" => Some("Render style:"),
        "MidiConsole 设置" => Some("MidiConsole Settings"),
        "下落速度:" => Some("Fall speed:"),
        "视图模式:" => Some("View mode:"),
        "Z 显示距离:" => Some("Z view distance:"),
        "3D 音符（盒子，关闭则为平面）" => Some("3D notes (boxes; flat when off)"),
        "分辨率:" => Some("Resolution:"),
        // ── 视频导出 · 下拉数据值的**显示**译名（`view/video_export_dialog/localized_choice.rs`）──
        // 这些中文同时是状态里存的规范数据值（并参与 match/==），只能在显示层用
        // `tr_dyn` 取词；见各 `pick_list` 的 `LocalizedChoice` 包装。
        // 渲染风格（`RenderMode::Display`）与质量（`QualityPreset::Display`）
        "Lumino瀑布流" => Some("Lumino Waterfall"),
        "Lumino卷帘" => Some("Lumino Roll"),
        "计数器" => Some("Counter"),
        "数据曲线" => Some("Data Curve"),
        "高" => Some("High"),
        "中" => Some("Medium"),
        "低" => Some("Low"),
        // 计数器对齐（`CounterAlignment::Display`）
        "左上" => Some("Top Left"),
        "右上" => Some("Top Right"),
        "左下" => Some("Bottom Left"),
        "右下" => Some("Bottom Right"),
        "顶部分散" => Some("Top Spread"),
        "底部分散" => Some("Bottom Spread"),
        // 字体来源（`state.counter_font_mode` / `state.dc_font_mode`）
        "内置点阵" => Some("Built-in Bitmap"),
        "系统字体" => Some("System Font"),
        "自定义字体" => Some("Custom Font"),
        // 数据曲线数据来源（`DataCurveMetric::Display`；「复音数」已在上方登记，复用）
        "NPS（每秒音符数）" => Some("NPS (notes per second)"),
        "累计音符数" => Some("Total Notes"),
        "BPM（速度）" => Some("BPM (tempo)"),
        _ => None,
    }
}
