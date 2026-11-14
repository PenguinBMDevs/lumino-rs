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
        // ── 手绘图形右键菜单（DRAW-019～028，`ui-editor/src/context_menu.rs`）──
        // 说明：这批文案在 HEAD 上已存在调用点但缺英文登记，i18n 覆盖率门禁为红；
        // 因该门禁同时守卫本卡新增文案，故一并补齐（纯登记，无行为变更）。
        "转为活形状" => Some("Convert to Live Shape"),
        "还原活形状" => Some("Revert Live Shape"),
        "连接图形" => Some("Join Shapes"),
        "还原连接" => Some("Split Joined Shape"),
        "门控合并" => Some("Gate-Merge Shapes"),
        "编辑形状" => Some("Edit Shape"),
        "竖轴对称" => Some("Symmetry: Vertical Axis"),
        "横轴对称" => Some("Symmetry: Horizontal Axis"),
        // ── 半边对称拒绝原因（DRAW-043，`ui-editor/src/interaction/shape_compose.rs`）──
        "请先选中一个形状" => Some("Select a shape first"),
        "请先选中一个待确认形状" => Some("Select a pending shape first"),
        "只有曲线形状支持半边对称" => Some("Only curve shapes support half symmetry"),
        "已连接的曲线不支持半边对称" => {
            Some("Joined curves do not support half symmetry")
        }
        "锚点太少，无法做半边对称" => Some("Too few anchors for half symmetry"),
        "锚点数为偶数，中间锚点需落在对称轴上" => {
            Some("Even anchor count: the middle anchor must lie on the axis")
        }
        "对称参数不合法" => Some("Invalid symmetry parameters"),
        "对称轴退化，无法求解" => Some("Degenerate symmetry axis"),
        "对称结果包含非有限值" => Some("Symmetry produced non-finite values"),
        "形状更新失败" => Some("Failed to update shape"),
        "扶正 50%" => Some("Straighten 50%"),
        "取消扶正" => Some("Clear Straightening"),
        "竖切" => Some("Slice Vertically"),
        "创建补间" => Some("Insert Tween"),
        "沿路径复制" => Some("Copy Along Path"),
        "力度渐强" => Some("Velocity Ramp Up"),
        "复制为分享文本" => Some("Copy as Share Text"),
        "从分享文本粘贴" => Some("Paste Share Text"),
        "输入文字…" => Some("Enter text…"),
        "设置内容区域" => Some("Settings content area"),
        "设置" => Some("Settings"),
        "不限制" => Some("Unlimited"),
        "0=不限制 1-128" => Some("0=Unlimited 1-128"),
        "警告" => Some("Warning"),
        "错误" => Some("Error"),
        "成功" => Some("Success"),
        "添加连接" => Some("Add Connection"),
        "尚未添加云存储连接。支持 FTP / SFTP / WebDAV，连接信息仅存储在本机。" => {
            Some(
                "No cloud storage connection yet. FTP / SFTP / WebDAV are supported; connection details are stored on this machine only.",
            )
        }
        "云存储管理" => Some("Cloud Storage"),
        "云存储提醒：{msg}" => Some("Cloud storage notice: {msg}"),
        "🟢 在线" => Some("🟢 Online"),
        "🔴 离线" => Some("🔴 Offline"),
        "断开" => Some("Disconnect"),
        "连接" => Some("Connect"),
        "管理文件" => Some("Manage Files"),
        "洋葱皮设置" => Some("Onion Skin Settings"),
        "洋葱皮渲染方式" => Some("Onion skin renderer"),
        // 工具栏洋葱皮下拉：显示范围（2026-11-05）
        "显示全部音轨" => Some("Show all tracks"),
        "显示上一个音轨" => Some("Show previous track"),
        "显示下一个音轨" => Some("Show next track"),
        "显示指定音轨…" => Some("Show specific track…"),
        "「指定音轨」的洋葱皮暂未接入，请先用全部 / 上一个 / 下一个" => {
            Some(
                "Onion skin for a specific track is not wired up yet; use All / Previous / Next for now",
            )
        }
        "加载 MIDI 后不可切换，请先关闭 MIDI 工程" => {
            Some("Cannot be changed after loading MIDI; close the MIDI project first")
        }
        "CPU 模式会禁用纵向卷帘、工程走带、瀑布流与视频导出" => Some(
            "CPU mode disables the vertical piano roll, project arrangement, waterfall and video export",
        ),
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
        // DRAW-041C：编辑器右键菜单的遮罩开关
        "切换遮罩" => Some("Toggle Mask"),
        _ => None,
    }
}
