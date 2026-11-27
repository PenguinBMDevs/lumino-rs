//! 兼容层词条（T-34b）：由 `lumino-extras` 旧静态表迁入。

/// 查表：未登记返回 `None`。
pub fn lookup(zh: &str) -> Option<&'static str> {
    lookup_legacy_1(zh)
        .or_else(|| lookup_legacy_2(zh))
        .or_else(|| lookup_legacy_3(zh))
        .or_else(|| lookup_legacy_4(zh))
}

fn lookup_legacy_1(zh: &str) -> Option<&'static str> {
    match zh {
        "BPM 速度" => Some("BPM Tempo"),
        "GPU 一次调度的音频帧数，须为 2 的幂（64–8192）；越小延迟越低但密集段落 GPU 开销越大" => {
            Some(
                "Audio frames rendered per GPU dispatch; must be a power of two (64–8192); smaller = lower latency but higher GPU cost for dense passages",
            )
        }
        "KDMAPI 模式使用系统驱动，无需音色库" => {
            Some("KDMAPI mode uses system driver, no soundfont needed")
        }
        "LGS (GPU): 基于 GPU 加速的渲染合成器，使用 lumino-gpu-synth 管线" => {
            Some("LGS (GPU): GPU-accelerated renderer backed by lumino-gpu-synth")
        }
        "LOAD 指标的显示方式：3 秒平均（稳定，默认）或瞬时（最近一块，抓尖峰，数值变化快）" => {
            Some(
                "How LOAD is displayed: 3s average (stable, default) or instant (last block; catches spikes, changes quickly)",
            )
        }
        "MIDI 已导出" => Some("MIDI exported"),
        "MIDI 输入设备:" => Some("MIDI Input Device:"),
        "System 模式使用系统默认的WinMM MIDI输出，无需音色库" => {
            Some("System mode uses default WinMM MIDI output, no soundfont needed")
        }
        "Tempo BPM 上限:" => Some("Tempo BPM Max:"),
        "Tempo 面板 BPM 绘制上限（默认 512）。速度点与刻度线按此范围映射，上限越大曲线越平坦。" => {
            Some(
                "Max BPM range for the Tempo panel (default 512). Tempo points and scale lines are mapped within this range; larger values flatten the curve.",
            )
        }
        "WinMM 输出设备 (播表):" => Some("WinMM Output Device (Playback List):"),
        "XSynth: 内置高性能合成器，支持SFZ/SF2格式音色库" => {
            Some("XSynth: Built-in high-performance synthesizer, supports SFZ/SF2 soundfonts")
        }
        "Y向框选工具" => Some("Y-Axis Box Select"),
        "一个高效的MIDI编辑工具" => Some("An efficient MIDI editor"),
        "与 Domino 互通剪贴板" => Some("Interoperate with Domino clipboard"),
        "两次自动保存的最小间隔（1-120，默认 5）。仅工程有未保存更改时才会写入。" => {
            Some(
                "Minimum interval between autosaves (1-120, default 5). Writes only while the project has unsaved changes.",
            )
        }
        "主题:" => Some("Theme:"),
        "云管理" => Some("Cloud Storage"),
        "交互" => Some("Interaction"),
        "仅 Pencil 连续绘制：在窗口内连续放置的音符合并为一个撤销日志（0=不合并，默认 300）" => {
            Some(
                "Pencil drawing only: notes placed within window merge into one undo log (0=disabled, default 300)",
            )
        }
        "仅 Windows 生效（当前平台无 Domino 剪贴板互通实现）" => {
            Some("Windows only (this platform has no Domino clipboard interoperability)")
        }
        "从云导入" => Some("Import from Cloud"),
        "从本地选取" => Some("Import from Local"),
        "位置：" => Some("Location: "),
        "低八度" => Some("Lower Octave"),
        "作者" => Some("Author"),
        "作者：" => Some("Author: "),
        "使用" => Some("Fill with"),
        "使用经典系统标题栏" => Some("Use native system title bar"),
        "保存到云" => Some("Save to Cloud"),
        "停止录制" => Some("Stop Recording"),
        "像素" => Some("px"),
        "像素 (从右边缘算起)" => Some("px (from right edge)"),
        "像素 (从左边缘算起)" => Some("px (from left edge)"),
        "全局最大复音数:" => Some("Global voice limit:"),
        "全局：曲线/形状描边与笔画都按它沿线段法线加粗；画布线宽 = 层数 × 单音高行高（随缩放）" => {
            Some(
                "Global: curve/shape outlines and strokes thicken along the segment normal; on-canvas stroke = layers x key height (follows zoom)",
            )
        }
        "关闭后不再执行启动检测；开启可用于诊断启动异常（防止死锁）" => {
            Some(
                "When disabled, startup checks are skipped; enable to diagnose startup issues (deadlock prevention)",
            )
        }
        "关闭后检测仍会执行，失败时仅在状态栏与日志中提示，不弹出警告窗" => {
            Some(
                "When disabled, checks still run; failures only appear in the status bar and logs without the warning window",
            )
        }
        "关闭填充" => Some("Disable fill"),
        "关闭时不存在任何 NoteOn 丢弃路径；开启后仅在负载持续超限时短暂限速，回落后自动解除" => {
            Some(
                "When off there is no NoteOn drop path at all; when on it briefly throttles only under sustained overload and releases automatically",
            )
        }
        "兼容性" => Some("Compatibility"),
        "内置合成器引擎:" => Some("Built-in Synth Engine:"),
        "内置素材" => Some("Built-in"),
        "冷却中，请稍候再试…" => Some("Cooling down, please try again shortly…"),
        "分割(Split)" => Some("Split"),
        "分音符" => Some("note"),
        "切换到瀑布流模式" => Some("Switch to Waterfall Mode"),
        "切换到编辑器模式" => Some("Switch to Editor Mode"),
        "切换循环播放" => Some("Toggle Loop Playback"),
        "切换洋葱皮显示" => Some("Toggle Onion Skin Display"),
        "切换自动滚动模式" => Some("Toggle Auto-Scroll Mode"),
        "创建日期" => Some("Created"),
        "删除后无法恢复，确定要删除吗？" => {
            Some("This cannot be undone. Delete anyway?")
        }
        "力度 0-127" => Some("Velocity 0-127"),
        "力度小于等于阈值的音符将不播放（0=关闭过滤）" => {
            Some("Notes with velocity <= threshold will not play (0=disabled)")
        }
        "力度过滤阈值" => Some("Velocity Filter Threshold"),
        "单条日志条目上限:" => Some("Single log entry limit:"),
        "只影响离线导出的 GPU 块长（512–8192，2 的幂）；越大渲染越快，但每块内存与尾延迟更高；实时播放不受影响" => {
            Some(
                "GPU block size for offline export only (512–8192, power of two); larger = faster but more memory/latency per block; realtime playback is unaffected",
            )
        }
        "合并(Glue)" => Some("Glue"),
        "合并窗口（毫秒）:" => Some("Merge window (ms):"),
        "合成器:" => Some("Synthesizer:"),
        "同键快速重复/密集和弦时，提高此值减少 voice stealing 导致的断音" => {
            Some("Increase for fast repeated notes / dense chords to reduce voice stealing")
        }
        "名称：" => Some("Name: "),
        "启动时显示 GPU 兼容性警告" => Some("Show GPU compatibility warning on startup"),
        "启用 256 键扩展钢琴卷帘" => Some("Enable 256-key extended piano roll"),
        "启用 HiDPI 图标渲染（推荐）" => {
            Some("Enable HiDPI icon rendering (recommended)")
        }
        "启用后，将使用系统原生标题栏，隐藏 Logo 和自定义窗口控制按钮" => {
            Some(
                "When enabled, uses the native OS title bar, hiding the custom logo and window controls",
            )
        }
        "启用周期自动保存" => Some("Enable periodic autosave"),
        "图片转MIDI" => Some("Image to MIDI"),
        "在 GPU 指标组内附加显示防爆治理级别与丢弃计数（仅异常时出现）" => {
            Some(
                "Append the anti-overload governor level and dropped-note count to the GPU group (only visible when non-zero)",
            )
        }
        "在底边栏显示 LGS (GPU) 的队列深度（秒+趋势箭头）、渲染负载、活跃声部与欠载计数；关闭后整组隐藏" => {
            Some(
                "Show LGS (GPU) queue depth (seconds + trend arrow), render load, active voices and underrun count in the bottom bar; disabling hides the whole group",
            )
        }
        "垂直翻转" => Some("Flip Vertical"),
        "复制诊断信息" => Some("Copy diagnostics"),
        "复音软目标比例:" => Some("Voice target ratio:"),
        "字体修改将立即生效" => Some("Font changes take effect immediately"),
        "导入文件" => Some("Import Files"),
        "导出为MIDI" => Some("Export as MIDI"),
        "导出为单文件" => Some("Export as Single File"),
        _ => None,
    }
}

fn lookup_legacy_2(zh: &str) -> Option<&'static str> {
    match zh {
        "导出为文件夹" => Some("Export as Folder"),
        "导出为素材" => Some("Export as Material"),
        "导出工程" => Some("Export Project"),
        "导出音频" => Some("Export Audio"),
        "尚未执行检测" => Some("No check performed yet"),
        "就绪" => Some("Ready"),
        "层" => Some("layers"),
        "工程信息设置" => Some("Project Settings"),
        "已加载，调色板锁定" => Some("loaded, palette locked"),
        "已复制" => Some("Copied"),
        "常规" => Some("General"),
        "底边栏 CPU/MEM/FPS 监控数据的刷新频率（50-2000ms，默认 100ms）" => Some(
            "Refresh interval for CPU/MEM/FPS monitoring data in the bottom bar (50-2000ms, default 100ms)",
        ),
        "开启后图标以2x分辨率渲染，在视网膜屏幕上更清晰。关闭可节省少量内存和渲染开销。" => {
            Some(
                "Icons render at 2x resolution for retina displays. Disable to save memory and rendering overhead.",
            )
        }
        "开启后复制会额外写入 Domino(MidiPortalSequence) 格式，粘贴可解析 Domino 数据；大选区复制会额外耗时并占用内存" => {
            Some(
                "When enabled, copying also writes the Domino (MidiPortalSequence) format and pasting can parse Domino data; large selections cost extra time and memory",
            )
        }
        "开启后钢琴卷帘拓展至 256 键 (0-255)，扩展区域（128-255）颜色略深以便区分。需要较强的 GPU 性能。" => {
            Some(
                "Extends piano roll to 256 keys (0-255). Extended range (128-255) has darker tint. Requires stronger GPU.",
            )
        }
        "开启时：过载（L2+）自动对 NoteOn 令牌桶限速（突发 50ms 成组放行，被拒音符的 NoteOff 配对抵消），保连续但会变薄；关闭时：完全放行，过载会周期性静音" => {
            Some(
                "On: under overload (L2+) NoteOns are token-bucket limited (50ms bursts pass as groups; the NoteOff of a dropped note is cancelled) - keeps audio continuous but thinner. Off: no limiting; overload causes periodic silence",
            )
        }
        "开始录制" => Some("Start Recording"),
        "引擎的复音硬上限（量程）。实际运行目标按下方比例留出暂态余量，负载反馈只会更低、不会更高；超限时从最忙通道抢占最老/最轻声部，新音符优先保留" => {
            Some(
                "Hard ceiling (range) for total voices. The runtime target leaves transient headroom via the ratio below; load feedback can only lower it. Over-limit steals the oldest/quietest voices from the busiest channels, keeping new notes audible",
            )
        }
        "形状工具：绘制矩形/椭圆等形状；可与颜料桶共存" => {
            Some("Shape Tool: draw rectangles/ellipses; can combine with paint bucket")
        }
        "循环播放: 关" => Some("Loop: Off"),
        "循环播放: 开" => Some("Loop: On"),
        "快捷键设置内容" => Some("Shortcut settings content"),
        "快进" => Some("Skip Forward"),
        "快退" => Some("Skip Backward"),
        "或" => Some("or"),
        "或输入字体文件路径..." => Some("or enter font file path..."),
        "打开" => Some("Open"),
        "打开协作面板" => Some("Open Collaboration Panel"),
        "打开工程文件" => Some("Open Project File"),
        "打开工程文件夹" => Some("Open Project Folder"),
        "打开设置" => Some("Open Settings"),
        "拦截时显示 Toast 提示" => Some("Show Toast on interception"),
        "按设定间隔把当前工程写入独立的自动保存文件（不影响手动保存的 .lmpj）。异常退出后下次启动可询问恢复；正常退出时自动清理。" => {
            Some(
                "Periodically writes the current project to a separate autosave file (your manually saved .lmpj is never touched). Kept after a crash and offered for recovery on next launch; cleaned up on normal exit.",
            )
        }
        "撤销" => Some("Undo"),
        "播放" => Some("Play"),
        "操作历史" => Some("Operation History"),
        "操作日志总条数上限:" => Some("Total history log limit:"),
        "放大" => Some("Zoom In"),
        "文件" => Some("File"),
        "文件已经保存" => Some("File saved"),
        "文字输入" => Some("Text Input"),
        "文字输入：在钢琴卷帘中插入文本标注；独立工具" => {
            Some("Text Input: insert text annotations in the piano roll; standalone tool")
        }
        "新建" => Some("New"),
        "新建工程" => Some("New Project"),
        "无可用设备" => Some("No devices available"),
        "无法预览选中调色板" => Some("Unable to preview selected palette"),
        "日志" => Some("Logging"),
        "日志文件保留份数:" => Some("Log file retention count:"),
        "日志文件存储在配置目录的 logs/ 下，超过此份数时自动删除最旧的日志（0 = 不限制）" => {
            Some(
                "Log files are stored in logs/ under the config directory. Oldest files are auto-deleted when exceeding this limit (0 = unlimited)",
            )
        }
        "显示防爆治理指标（GOV/DROP）" => Some("Show anti-overload metrics (GOV/DROP)"),
        "暂无最近文件" => Some("No recent files"),
        "曲线工具：绘制连续音符音高曲线，可叠加颜料桶填充" => {
            Some("Curve Tool: draw continuous pitch curve; can combine with paint bucket fill")
        }
        "更多工具" => Some("More Tools"),
        "最近打开" => Some("Recent Files"),
        "未检测到可用的 WinMM 输出设备" => {
            Some("No available WinMM output device detected")
        }
        "未检测到可用的音频播放输出设备" => {
            Some("No available audio playback output device detected")
        }
        "本地素材" => Some("Local"),
        "来源：" => Some("Source: "),
        "查找" => Some("Find"),
        "框选框模式:" => Some("Selection Box Mode:"),
        "检查 GPU 兼容性" => Some("Check GPU compatibility"),
        "检查中…" => Some("Checking…"),
        "检测失败" => Some("Check failed"),
        "检测已连续超时，本次运行不再重试；请重启应用后再检查。" => {
            Some(
                "The check timed out repeatedly; retrying is disabled until the app restarts. Restart and check again.",
            )
        }
        "检测未在时限内完成，驱动可能仍在后台挂起；若界面持续卡顿请重启应用。" => {
            Some(
                "The check did not finish in time; the driver may still be hung in the background. Restart the app if the UI keeps stuttering.",
            )
        }
        "检测超时（驱动可能挂起）" => Some("Check timed out (the driver may be hung)"),
        "检测通过" => Some("Check passed"),
        "模式1 - 指示线固定位置:" => Some("Mode 1 - Fixed indicator position:"),
        "模式2 - 翻页后位置:" => Some("Mode 2 - Page return position:"),
        "模式2 - 翻页触发位置:" => Some("Mode 2 - Page trigger offset:"),
        "橡皮擦" => Some("Eraser"),
        "橡皮擦行为:" => Some("Eraser Behavior:"),
        "橡皮擦：擦除音符；独立工具" => Some("Eraser: erase notes; standalone tool"),
        "每次启动时检查 GPU 兼容性" => Some("Check GPU compatibility on every startup"),
        "每键最大同音数:" => Some("Max voices per key:"),
        "水平翻转" => Some("Flip Horizontal"),
        "没有可导出的 MIDI 内容" => Some("No MIDI content to export"),
        "洋葱皮" => Some("Onion Skin"),
        "添加素材" => Some("Add Material"),
        "添加音轨" => Some("Add Track"),
        _ => None,
    }
}

fn lookup_legacy_3(zh: &str) -> Option<&'static str> {
    match zh {
        "添加音轨时:" => Some("Add Track Behavior:"),
        "清除最近记录" => Some("Clear Recent"),
        "瀑布流" => Some("Waterfall"),
        "版本" => Some("Version"),
        "版权信息" => Some("Copyright"),
        "状态栏显示 GPU 缓冲状态（默认开启）" => {
            Some("Show GPU buffer status in the status bar (on by default)")
        }
        "界面" => Some("UI"),
        "留空 = 整块填充（不切分）" => Some("Empty = single block (no split)"),
        "监控数据刷新间隔:" => Some("Monitor refresh interval:"),
        "直接框选: 拖动框选删除，Shift+点击删除单个" => {
            Some("Direct Select: drag to box-delete, Shift+click to delete single")
        }
        "直接跟随：框选框实时跟随鼠标，响应最即时。弹簧动画：框选框边界带有弹性动画效果，视觉更生动。" => {
            Some(
                "Direct: selection box follows cursor instantly. Spring: selection box has elastic animation for a lively feel.",
            )
        }
        "确认" => Some("Confirm"),
        "离线导出块长 (samples):" => Some("Offline Export Block Size (samples):"),
        "移调 +1" => Some("Transpose +1"),
        "移调 -1" => Some("Transpose -1"),
        "程序字体:" => Some("Program Font:"),
        "粗细设置" => Some("Stroke Width"),
        "精度:" => Some("Precision:"),
        "系统字体与自定义字体文件二选一（文件路径优先）。字体加载失败时回退默认字体；修改后立即生效。" => {
            Some(
                "Choose either a system font or a custom font file (file path takes priority). Falls back to default if loading fails; changes take effect immediately.",
            )
        }
        "系统默认输出设备" => Some("System Default Output Device"),
        "素材导入失败" => Some("Failed to import material"),
        "素材库" => Some("Material Library"),
        "素材无效" => Some("Invalid material"),
        "累计创作时间" => Some("Total Editing Time"),
        "缓冲区 (延迟)" => Some("Buffer (Latency)"),
        "缓冲区大小 (samples):" => Some("Buffer Size (samples):"),
        "编辑中触发 Undo/Redo/Save/Play/Export 时显示提示（关闭则静默处理）" => {
            Some(
                "Show notification when Undo/Redo/Save/Play/Export is intercepted during editing (disable for silent handling)",
            )
        }
        "编辑器" => Some("Editor"),
        "编辑拦截" => Some("Edit Interception"),
        "缩小" => Some("Zoom Out"),
        "自动保存" => Some("Autosave"),
        "自动保存间隔（分钟）:" => Some("Autosave interval (minutes):"),
        "自动化曲线连线粗细:" => Some("Automation line thickness:"),
        "自动化面板中事件瞄点之间的连线粗细（1-10 像素）" => {
            Some("Line thickness between event anchor points in the automation panel (1-10 pixels)")
        }
        "自动滚动: 关闭" => Some("Auto-Scroll: Off"),
        "自动滚动: 固定" => Some("Auto-Scroll: Fixed"),
        "自动滚动: 滚动" => Some("Auto-Scroll: Scroll"),
        "自动滚动设置" => Some("Auto-Scroll Settings"),
        "自由笔画" => Some("Free Stroke"),
        "自由笔画：拖出矢量笔画（实时预览），松手后 √ 一次性生成音符；独立工具，不可与颜料桶共存" => {
            Some(
                "Free Stroke: drag out vector strokes with live preview, confirm with √ to create notes; standalone tool, cannot combine with paint bucket",
            )
        }
        "视图" => Some("View"),
        "设置卷帘自动滚动时演奏指示线的位置行为" => {
            Some("Configure auto-scroll behavior for the playhead in the piano roll")
        }
        "设置项开发中：后续在此聚合描边、画笔、形状与颜料桶等绘制参数。" => {
            Some(
                "Settings in development: stroke, brush, shape and paint bucket options will be gathered here.",
            )
        }
        "调色板文件存放在 resources/palettes/ 目录下，编译时自动检测。添加新的 PNG 文件后重新编译即可使用。" => {
            Some(
                "Palette files are stored in resources/palettes/ and auto-detected at compile time. Add new PNG files and rebuild to use them.",
            )
        }
        "负载显示口径:" => Some("Load display:"),
        "超过此值时弹出最早的日志（默认 100，建议 50-200）" => {
            Some("Oldest log is evicted when exceeded (default 100, recommended 50-200)")
        }
        "超过此值时强制分割为新日志（默认 1000，建议 500-2000）" => {
            Some("Auto-split into new log when exceeded (default 1000, recommended 500-2000)")
        }
        "跟随系统" => Some("Follow System"),
        "轨道数：" => Some("Tracks: "),
        "输入 BPM 上限（如 700）" => Some("Enter BPM max (e.g. 700)"),
        "输入作者（可选）" => Some("Enter author (optional)"),
        "输入版权信息（可选）" => Some("Enter copyright info (optional)"),
        "输入项目名称（留空显示为'无标题'）" => {
            Some("Enter project name (blank = 'Untitled')")
        }
        "过载保命闸（仅在重度过载时临时限速，默认关闭）" => {
            Some("Overload safety gate (temporary throttling under heavy overload, off by default)")
        }
        "运行目标 = 比例 × 硬上限（默认 1-1/e≈0.632，留约 37% 暂态余量；1-1/e²≈0.865 更激进但余量仅 13.5%）" => {
            Some(
                "Runtime target = ratio x hard limit (default 1-1/e ~= 0.632, ~37% transient headroom; 1-1/e^2 ~= 0.865 is more aggressive but leaves only 13.5%)",
            )
        }
        "连奏(Tie)" => Some("Tie (Legato)"),
        "连接到多人协作房间" => Some("Connect to Collaboration Room"),
        "退出" => Some("Exit"),
        "选择MIDI设备" => Some("Select MIDI device"),
        "选择工具" => Some("Select Tool"),
        "选择添加音轨后是否自动跳转到新音轨" => {
            Some("Choose whether to auto-switch to the newly added track")
        }
        "选择精度" => Some("Select precision"),
        "选择调色板:" => Some("Select Palette:"),
        "选择音符总量的展示位置：工程设置对话框中，或下边栏（空闲时顶替「就绪」；悬停解释与状态消息优先显示）。" => {
            Some(
                "Where to show the total note count: in the project settings dialog, or in the status bar (replaces \"Ready\" when idle; hover tooltips and status messages take priority).",
            )
        }
        "选择音色库文件 (SFZ/SF2)..." => Some("Select soundfont file (SFZ/SF2)..."),
        "选择音频播放输出设备；默认使用系统默认设备。更改后下次播放（软件合成器重建）生效" => {
            Some(
                "Select the audio playback output device; system default is used if unset. Applies on next playback (software synth rebuild)",
            )
        }
        "速度 BPM" => Some("Tempo BPM"),
        "重做" => Some("Redo"),
        "重置缩放" => Some("Reset Zoom"),
        "量化" => Some("Quantize"),
        "钢琴瀑布流" => Some("Piano Waterfall"),
        "铅笔工具" => Some("Pencil Tool"),
        "防爆闸（LGS，默认开启）" => Some("Anti-overload gate (LGS, on by default)"),
        "除以" => Some("divide by"),
        "音符总量" => Some("Total Notes"),
        "音符总量显示位置" => Some("Note count display"),
        "音符画工具箱" => Some("Note Drawing Toolbox"),
        _ => None,
    }
}

fn lookup_legacy_4(zh: &str) -> Option<&'static str> {
    match zh {
        "音色库:" => Some("Soundfont:"),
        "音轨列表" => Some("Track List"),
        "音频播放输出设备:" => Some("Audio Playback Output Device:"),
        "项目名称" => Some("Project Name"),
        "颜料桶" => Some("Paint Bucket"),
        "颜料桶：填充封闭区域；可随时切换，仅作用于曲线/形状绘制的封闭图形" => {
            Some(
                "Paint Bucket: fill enclosed regions; toggle anytime, only applies to closed shapes drawn by Curve/Shape",
            )
        }
        "颜色预览" => Some("Color Preview"),
        "高八度" => Some("Raise Octave"),
        "高对比度" => Some("High Contrast"),
        "默认: Shift+拖动框选删除，点击删除单个" => {
            Some("Default: Shift+drag to box-delete, click to delete single")
        }
        "鼠标工具" => Some("Mouse Tool"),
        "鼠标工具：点选已绘制的图形，选中后高亮描边" => Some(
            "Mouse Tool: click a drawn shape to select it; selected shapes get a highlight outline",
        ),
        _ => None,
    }
}
