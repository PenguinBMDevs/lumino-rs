//! 文案目录分域：crates/editor/{ui-core,ui-editor,ui-settings,editor-state}/**（UI 基础件与设置页）
//!
//! 维护约定见 `catalog.rs` 顶部：一行一条 `"中文原文" => Some("English"),`，
//! 命名占位符 `{name}` 两种语言必须完全一致（由 `catalog.rs` 的测试强制）。

/// 查本域英文译文；未登记返回 `None`。
pub fn lookup(zh: &str) -> Option<&'static str> {
    match zh {
        "导出完成" => Some("Export complete"),
        "导出失败: {error}" => Some("Export failed: {error}"),
        "不足 1 秒" => Some("Less than 1 second"),
        "{days} 天 {hours} 小时 {minutes} 分钟" => Some("{days} d {hours} h {minutes} min"),
        "{hours} 小时 {minutes} 分钟" => Some("{hours} h {minutes} min"),
        "{minutes} 分钟 {seconds} 秒" => Some("{minutes} min {seconds} s"),
        "{seconds} 秒" => Some("{seconds} s"),
        "用户" => Some("User"),
        "我的房间" => Some("My Room"),
        "剪切" => Some("Cut"),
        "复制" => Some("Copy"),
        "粘贴" => Some("Paste"),
        "删除" => Some("Delete"),
        "全选" => Some("Select All"),
        "批量编辑" => Some("Batch Edit"),
        "输入文字…" => Some("Enter text…"),
        "设置内容区域" => Some("Settings content area"),
        "设置" => Some("Settings"),
        "不限制" => Some("Unlimited"),
        "0=不限制 1-128" => Some("0=Unlimited 1-128"),
        "⚠ 云存储提醒：{msg}" => Some("⚠ Cloud storage notice: {msg}"),
        "添加连接" => Some("Add Connection"),
        "尚未添加云存储连接。支持 FTP / SFTP / WebDAV，连接信息仅存储在本机。" => {
            Some(
                "No cloud storage connection yet. FTP / SFTP / WebDAV are supported; connection details are stored on this machine only.",
            )
        }
        "云存储管理" => Some("Cloud Storage"),
        "🟢 在线" => Some("🟢 Online"),
        "🔴 离线" => Some("🔴 Offline"),
        "断开" => Some("Disconnect"),
        "连接" => Some("Connect"),
        "管理文件" => Some("Manage Files"),
        "洋葱皮设置" => Some("Onion Skin Settings"),
        "启用高精度洋葱皮贴图" => Some("Enable high-precision onion skin tiles"),
        "启用后将在钢琴卷帘上叠加高精度预渲染贴图，编辑后自动重生成。" => {
            Some(
                "When enabled, high-precision prerendered tiles are overlaid on the piano roll and regenerated after edits.",
            )
        }
        "每组小节数 (1-16)" => Some("Measures per group (1-16)"),
        "组内小节数越大单张贴图越长，内存占用越高。" => {
            Some("More measures per group means longer tiles and higher memory usage.")
        }
        "贴图宽度像素 (480-7680)" => Some("Tile width in pixels (480-7680)"),
        "贴图宽度越大越清晰但内存占用越高，建议不超过 3840。" => {
            Some("Larger tiles are sharper but use more memory; 3840 or less is recommended.")
        }
        "编辑后重生成冷静期秒数 (3-60)" => {
            Some("Regeneration cooldown after edits, seconds (3-60)")
        }
        "停止编辑后等待指定秒数再重生成贴图，避免频繁重绘。" => Some(
            "Wait this many seconds after editing stops before regenerating tiles, avoiding frequent redraws.",
        ),
        "GPU 显存上限 MB (128-4096)" => Some("GPU memory limit MB (128-4096)"),
        "GPU 显存上限越大可同时显示越多贴图。" => {
            Some("A higher GPU memory limit allows more tiles to be shown at once.")
        }
        "语言 / Language:" => Some("Language:"),
        "选择系统字体..." => Some("Select a system font..."),
        "力度面板曲线显示（默认）" => Some("Show velocity panel as curve (default)"),
        "关闭后使用柱状图显示力度值" => {
            Some("When disabled, velocity values are shown as bars")
        }
        "播放时键盘颜色指示（默认关闭）" => {
            Some("Keyboard color indication during playback (off by default)")
        }
        "开启后播放时在钢琴键盘上高亮当前音符位置，占用额外内存" => {
            Some(
                "When enabled, the current note position is highlighted on the piano keyboard during playback, using extra memory",
            )
        }
        "音色库文件" => Some("Soundfont Files"),
        "SF2 文件" => Some("SF2 Files"),
        "SFZ 文件" => Some("SFZ Files"),
        "所有文件" => Some("All Files"),
        "字体文件" => Some("Font Files"),
        "TrueType 字体" => Some("TrueType Fonts"),
        "OpenType 字体" => Some("OpenType Fonts"),
        // 工具栏按钮描述表未命中时的兜底说明（`ui-core/src/button_descs.rs`）
        "未配置说明" => Some("No description configured"),
        _ => None,
    }
}
