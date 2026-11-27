//! `ui_view.rs` 的后半段词条（按原 match 顺序切分）。

pub(super) fn lookup(zh: &str) -> Option<&'static str> {
    lookup_view_6(zh)
        .or_else(|| lookup_view_7(zh))
        .or_else(|| lookup_view_8(zh))
        .or_else(|| lookup_view_9(zh))
}

fn lookup_view_6(zh: &str) -> Option<&'static str> {
    match zh {
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
        _ => None,
    }
}

fn lookup_view_7(zh: &str) -> Option<&'static str> {
    match zh {
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
        _ => None,
    }
}

fn lookup_view_8(zh: &str) -> Option<&'static str> {
    match zh {
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

        // ── 视频导出 · 渲染设置（`view/video_export_dialog/render_settings.rs`）──
        "渲染设置" => Some("Render Settings"),

        "帧率:" => Some("Frame rate:"),

        "渲染格式:" => Some("Render format:"),

        "编码器:" => Some("Encoder:"),

        "加速:" => Some("Acceleration:"),

        "质量:" => Some("Quality:"),

        "渲染风格:" => Some("Render style:"),

        "MidiConsole 设置" => Some("MidiConsole Settings"),
        _ => None,
    }
}

fn lookup_view_9(zh: &str) -> Option<&'static str> {
    match zh {
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

        // ── REND-005：端口音色库对话框与工程设置入口 ──
        // （`root/view/egui_chrome/dialogs/port_soundfonts*.rs`、`project_settings.rs`）
        "为每个 MIDI 端口绑定音色库；未配置的端口使用全局音色库，端口 0 可覆盖全局。播放中保存的修改将在停播后生效。" => {
            Some(
                "Bind a soundfont to each MIDI port; unconfigured ports use the global soundfont, and port 0 may override the global. Changes saved during playback take effect after playback stops.",
            )
        }
        "全部使用全局音色库" => Some("All ports use the global soundfont"),

        "另有 {n} 个回退库（当前 UI 只显示首个）" => {
            Some("{n} additional fallback soundfonts (the UI currently shows only the first)")
        }
        "已保存 {ports} 个端口的映射；当前播放与导出将使用全局音色库（降级运行）。切换到 XSynth 后可恢复按端口音色库。" => {
            Some(
                "Saved mappings for {ports} ports; playback and export currently use the global soundfont (degraded). Switch to XSynth to restore per-port soundfonts.",
            )
        }
        "已配置 {n} 个端口" => Some("{n} ports configured"),

        "当前后端「{backend}」不支持按端口音色库。" => {
            Some("The current backend “{backend}” does not support per-port soundfonts.")
        }
        "未覆盖：使用全局音色库" => Some("Not overridden: uses the global soundfont"),

        "未配置：回退全局音色库" => {
            Some("Not configured: falls back to the global soundfont")
        }

        "清除" => Some("Clear"),

        "端口 0（全局，默认，可覆盖）" => {
            Some("Port 0 (global, default, can be overridden)")
        }
        "端口 {port}" => Some("Port {port}"),

        "端口音色库" => Some("Port Soundfonts"),

        "继续（使用全局音色库）" => Some("Continue (use the global soundfont)"),

        "编辑…" => Some("Edit…"),

        "选择…" => Some("Choose…"),
        _ => None,
    }
}
