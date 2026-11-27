//! 文案目录分域：`crates/editor/ui/src/view/**`（各对话框正文）
//!
//! 维护约定见 `catalog.rs` 顶部：一行一条 `"中文原文" => Some("English"),`，
//! 命名占位符 `{name}` 两种语言必须完全一致（由 `catalog.rs` 的测试强制）。

/// 查本域英文译文；未登记返回 `None`。
pub fn lookup(zh: &str) -> Option<&'static str> {
    lookup_view_1(zh)
        .or_else(|| lookup_view_2(zh))
        .or_else(|| lookup_view_3(zh))
        .or_else(|| lookup_view_4(zh))
        .or_else(|| lookup_view_5(zh))
        .or_else(|| tail::lookup(zh))
}

fn lookup_view_1(zh: &str) -> Option<&'static str> {
    match zh {
        "帮助" => Some("Help"),

        "搜索帮助..." => Some("Search help..."),

        "没有匹配的话题" => Some("No matching topics"),

        "全部重现" => Some("Show all again"),

        "查看帮助" => Some("View help"),

        "知道了" => Some("Got it"),

        "区域着色" => Some("Paint areas"),

        "区域着色已开启：点击形状内部上色，切工具退出" => {
            Some("Area painting on: click inside the shape to paint; switch tools to exit")
        }
        "区域着色失败：请选中一个待确认闭合形状" => {
            Some("Area painting failed: select a pending closed shape")
        }
        "形状列表" => Some("Shape list"),

        "形状列表（只读）" => Some("Shape list (read-only)"),

        "形状库目录" => Some("Shape library folder"),

        "尚未注入形状库目录" => Some("Shape library folder not injected"),

        "画板与工具将在 033B1 接入" => Some("Canvas and tools will arrive in 033B1"),

        "形状" => Some("Shape"),

        "曲线" => Some("Curve"),

        "折线" => Some("Polyline"),

        // ── 撤销历史面板（`view/history_dialog.rs`）──
        "撤销历史" => Some("Undo History"),

        "初始状态" => Some("Initial state"),

        "新建音符" => Some("Create note"),

        "移动音符" => Some("Move note"),

        "删除音符" => Some("Delete note"),

        "变换音符" => Some("Transform note"),

        "调整力度" => Some("Adjust velocity"),

        "自动化编辑" => Some("Edit automation"),

        "MIDI 录制" => Some("MIDI recording"),

        "形状编辑" => Some("Shape edit"),

        "其他操作" => Some("Other action"),

        "点击步骤跳转；初始状态为 0。" => {
            Some("Click a step to jump; the initial state is 0.")
        }
        "点击步骤回到该步完成后的状态；初始状态为 0。" => {
            Some("Click a step to return to its completed state; the initial state is 0.")
        }
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
        _ => None,
    }
}

fn lookup_view_2(zh: &str) -> Option<&'static str> {
    match zh {
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
        _ => None,
    }
}

fn lookup_view_3(zh: &str) -> Option<&'static str> {
    match zh {
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
        "已设为不限：密集素材下声部会无限堆积，渲染时间与内存会显著恶化——除非明确需要，建议改回 32。" => {
            Some(
                "Unlimited: voices pile up endlessly with dense material and render time and memory degrade sharply — set it back to 32 unless you really need this.",
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

        "请先选择视频输出路径" => Some("Choose a video output path first"),
        _ => None,
    }
}

fn lookup_view_4(zh: &str) -> Option<&'static str> {
    match zh {
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
        _ => None,
    }
}

fn lookup_view_5(zh: &str) -> Option<&'static str> {
    match zh {
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
        _ => None,
    }
}

mod tail;
