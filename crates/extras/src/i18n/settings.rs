//! 设置面板翻译

use super::Language;

/// 设置面板翻译
#[derive(Debug, Clone)]
pub struct SettingsTranslations {
    // ── 菜单项 ──
    pub general: &'static str,
    pub audio: &'static str,
    pub ui: &'static str,
    pub shortcuts: &'static str,
    pub about: &'static str,
    pub onion_skin: &'static str,
    pub palette: &'static str,
    pub editing: &'static str,

    // ── 通用 ──
    pub confirm: &'static str,
    pub cancel: &'static str,
    pub ok: &'static str,
    pub or: &'static str,
    pub browse: &'static str,

    // ── 常规页面 ──
    pub general_title: &'static str,
    pub eraser_behavior: &'static str,
    pub eraser_default_hint: &'static str,
    pub eraser_direct_hint: &'static str,
    pub track_add_behavior: &'static str,
    pub track_add_behavior_hint: &'static str,

    // ── 音频页面 ──
    pub audio_title: &'static str,
    pub synthesizer: &'static str,
    pub soundfont: &'static str,
    pub soundfont_placeholder: &'static str,
    pub buffer_latency: &'static str,
    pub fade_out_label: &'static str,
    pub max_voices: &'static str,
    pub max_voices_hint: &'static str,
    pub velocity_filter: &'static str,
    pub velocity_filter_hint: &'static str,
    pub midi_input_device: &'static str,
    pub no_device: &'static str,
    pub select_device_placeholder: &'static str,
    pub kdmapi_hint: &'static str,
    pub system_hint: &'static str,
    pub xsynth_hint: &'static str,

    // ── 界面页面 ──
    pub ui_title: &'static str,
    pub theme: &'static str,
    pub program_font: &'static str,
    pub font_path_placeholder: &'static str,
    pub font_hint: &'static str,
    pub native_titlebar: &'static str,
    pub native_titlebar_hint: &'static str,
    pub hidpi_icon: &'static str,
    pub hidpi_icon_hint: &'static str,
    pub auto_scroll: &'static str,
    pub auto_scroll_fixed: &'static str,
    pub auto_scroll_trigger: &'static str,
    pub auto_scroll_return: &'static str,
    pub auto_scroll_hint: &'static str,
    pub interaction: &'static str,
    pub selection_box_mode: &'static str,
    pub selection_box_hint: &'static str,
    pub piano_roll: &'static str,
    pub enable_256key: &'static str,
    pub enable_256key_hint: &'static str,
    pub pixel: &'static str,
    pub from_left: &'static str,
    pub from_right: &'static str,

    // ── 快捷键页面 ──
    pub shortcuts_title: &'static str,
    pub shortcuts_placeholder: &'static str,

    // ── 关于页面 ──
    pub about_title: &'static str,
    pub app_name: &'static str,

    // ── 调色板页面 ──
    pub palette_title: &'static str,
    pub palette_select: &'static str,
    pub palette_hint: &'static str,
    pub palette_colors_info: &'static str,
    pub palette_no_preview: &'static str,
    pub palette_locked: &'static str,
    pub version: &'static str,
    pub app_description: &'static str,
    /// 高对比度主题显示名称
    pub high_contrast: &'static str,

    // ── 编辑页面 ──
    pub editing_title: &'static str,
    pub editing_history_section: &'static str,
    pub editing_history_total_limit: &'static str,
    pub editing_history_total_limit_hint: &'static str,
    pub editing_history_entry_limit: &'static str,
    pub editing_history_entry_limit_hint: &'static str,
    pub editing_merge_window: &'static str,
    pub editing_merge_window_hint: &'static str,
    pub editing_intercept_section: &'static str,
    pub editing_intercept_notification: &'static str,
    pub editing_intercept_notification_hint: &'static str,
    /// Tempo 面板 BPM 上限
    pub editing_tempo_max_bpm: &'static str,
    pub editing_tempo_max_bpm_hint: &'static str,
    /// 自定义 BPM 上限弹窗
    pub editing_tempo_custom_option: &'static str,
    pub editing_tempo_custom_title: &'static str,
    pub editing_tempo_custom_placeholder: &'static str,
    /// 自动化曲线连线粗细
    pub ui_automation_line_thickness: &'static str,
    pub ui_automation_line_thickness_hint: &'static str,
    /// 日志存储份数
    pub log_retention_section: &'static str,
    pub log_retention_count: &'static str,
    pub log_retention_count_hint: &'static str,
    /// 底边栏监控数据刷新间隔
    pub ui_monitor_refresh_interval: &'static str,
    pub ui_monitor_refresh_interval_hint: &'static str,
}

static ZHCN_SETTINGS: SettingsTranslations = SettingsTranslations {
    general: "常规",
    audio: "音频",
    ui: "界面",
    shortcuts: "快捷键",
    about: "关于",
    onion_skin: "洋葱皮",
    palette: "调色板",
    editing: "编辑",
    confirm: "确认",
    cancel: "取消",
    ok: "确定",
    or: "或",
    browse: "浏览...",
    general_title: "常规",
    eraser_behavior: "橡皮擦行为:",
    eraser_default_hint: "默认: Shift+拖动框选删除，点击删除单个",
    eraser_direct_hint: "直接框选: 拖动框选删除，Shift+点击删除单个",
    track_add_behavior: "添加音轨时:",
    track_add_behavior_hint: "选择添加音轨后是否自动跳转到新音轨",
    audio_title: "音频",
    synthesizer: "合成器:",
    soundfont: "音色库:",
    soundfont_placeholder: "选择音色库文件 (SFZ/SF2)...",
    buffer_latency: "缓冲区 (延迟)",
    fade_out_label: "释放音符时平滑淡出 (防止爆音)",
    max_voices: "每键最大同音数:",
    max_voices_hint: "同键快速重复/密集和弦时，提高此值减少 voice stealing 导致的断音",
    velocity_filter: "力度过滤阈值",
    velocity_filter_hint: "力度小于等于阈值的音符将不播放（0=关闭过滤）",
    midi_input_device: "MIDI 输入设备:",
    no_device: "无可用设备",
    select_device_placeholder: "选择MIDI设备",
    kdmapi_hint: "KDMAPI 模式使用系统驱动，无需音色库",
    system_hint: "System 模式使用系统默认的WinMM MIDI输出，无需音色库",
    xsynth_hint: "XSynth: 内置高性能合成器，支持SFZ/SF2格式音色库",
    ui_title: "界面",
    theme: "主题:",
    program_font: "程序字体:",
    font_path_placeholder: "或输入字体文件路径...",
    font_hint: "选择系统字体或指定自定义字体文件路径。如果自定义路径无效，将回退到默认字体。",
    native_titlebar: "使用经典系统标题栏",
    native_titlebar_hint: "启用后，将使用系统原生标题栏，隐藏 Logo 和自定义窗口控制按钮",
    hidpi_icon: "启用 HiDPI 图标渲染（推荐）",
    hidpi_icon_hint: "开启后图标以2x分辨率渲染，在视网膜屏幕上更清晰。关闭可节省少量内存和渲染开销。",
    auto_scroll: "自动滚动设置",
    auto_scroll_fixed: "模式1 - 指示线固定位置:",
    auto_scroll_trigger: "模式2 - 翻页触发位置:",
    auto_scroll_return: "模式2 - 翻页后位置:",
    auto_scroll_hint: "设置卷帘自动滚动时演奏指示线的位置行为",
    interaction: "交互",
    selection_box_mode: "框选框模式:",
    selection_box_hint: "直接跟随：框选框实时跟随鼠标，响应最即时。弹簧动画：框选框边界带有弹性动画效果，视觉更生动。",
    piano_roll: "钢琴卷帘",
    enable_256key: "启用 256 键扩展钢琴卷帘",
    enable_256key_hint: "开启后钢琴卷帘拓展至 256 键 (0-255)，扩展区域（128-255）颜色略深以便区分。需要较强的 GPU 性能。",
    pixel: "像素",
    from_left: "像素 (从左边缘算起)",
    from_right: "像素 (从右边缘算起)",
    shortcuts_title: "快捷键",
    shortcuts_placeholder: "快捷键设置内容",
    about_title: "关于",
    app_name: "Lumino",
    version: "版本 0.1.1-dev",
    app_description: "一个高效的MIDI编辑工具",
    high_contrast: "高对比度",
    palette_title: "调色板",
    palette_select: "选择调色板:",
    palette_hint: "调色板文件存放在 resources/palettes/ 目录下，编译时自动检测。添加新的 PNG 文件后重新编译即可使用。",
    palette_colors_info: "颜色预览",
    palette_no_preview: "无法预览选中调色板",
    palette_locked: "已加载，调色板锁定",
    editing_title: "编辑",
    editing_history_section: "操作历史",
    editing_history_total_limit: "操作日志总条数上限:",
    editing_history_total_limit_hint: "超过此值时弹出最早的日志（默认 100，建议 50-200）",
    editing_history_entry_limit: "单条日志条目上限:",
    editing_history_entry_limit_hint: "超过此值时强制分割为新日志（默认 1000，建议 500-2000）",
    editing_merge_window: "合并窗口（毫秒）:",
    editing_merge_window_hint: "仅 Pencil 连续绘制：在窗口内连续放置的音符合并为一个撤销日志（0=不合并，默认 300）",
    editing_intercept_section: "编辑拦截",
    editing_intercept_notification: "拦截时显示 Toast 提示",
    editing_intercept_notification_hint: "编辑中触发 Undo/Redo/Save/Play/Export 时显示提示（关闭则静默处理）",
    editing_tempo_max_bpm: "Tempo BPM 上限:",
    editing_tempo_max_bpm_hint: "Tempo 面板 BPM 绘制上限（默认 512）。速度点与刻度线按此范围映射，上限越大曲线越平坦。",
    editing_tempo_custom_option: "自定义",
    editing_tempo_custom_title: "自定义 BPM 上限",
    editing_tempo_custom_placeholder: "输入 BPM 上限（如 700）",
    ui_automation_line_thickness: "自动化曲线连线粗细:",
    ui_automation_line_thickness_hint: "自动化面板中事件瞄点之间的连线粗细（1-10 像素）",
    ui_monitor_refresh_interval: "监控数据刷新间隔:",
    ui_monitor_refresh_interval_hint: "底边栏 CPU/MEM/FPS 监控数据的刷新频率（50-2000ms，默认 100ms）",
    log_retention_section: "日志",
    log_retention_count: "日志文件保留份数:",
    log_retention_count_hint: "日志文件存储在配置目录的 logs/ 下，超过此份数时自动删除最旧的日志（0 = 不限制）",
};

static ENUS_SETTINGS: SettingsTranslations = SettingsTranslations {
    general: "General",
    audio: "Audio",
    ui: "UI",
    shortcuts: "Shortcuts",
    about: "About",
    onion_skin: "Onion Skin",
    palette: "Palette",
    editing: "Editing",
    confirm: "Confirm",
    cancel: "Cancel",
    ok: "OK",
    or: "or",
    browse: "Browse...",
    general_title: "General",
    eraser_behavior: "Eraser Behavior:",
    eraser_default_hint: "Default: Shift+drag to box-delete, click to delete single",
    eraser_direct_hint: "Direct Select: drag to box-delete, Shift+click to delete single",
    track_add_behavior: "Add Track Behavior:",
    track_add_behavior_hint: "Choose whether to auto-switch to the newly added track",
    audio_title: "Audio",
    synthesizer: "Synthesizer:",
    soundfont: "Soundfont:",
    soundfont_placeholder: "Select soundfont file (SFZ/SF2)...",
    buffer_latency: "Buffer (Latency)",
    fade_out_label: "Smooth fade-out on note release (prevent clicks)",
    max_voices: "Max voices per key:",
    max_voices_hint: "Increase for fast repeated notes / dense chords to reduce voice stealing",
    velocity_filter: "Velocity Filter Threshold",
    velocity_filter_hint: "Notes with velocity <= threshold will not play (0=disabled)",
    midi_input_device: "MIDI Input Device:",
    no_device: "No devices available",
    select_device_placeholder: "Select MIDI device",
    kdmapi_hint: "KDMAPI mode uses system driver, no soundfont needed",
    system_hint: "System mode uses default WinMM MIDI output, no soundfont needed",
    xsynth_hint: "XSynth: Built-in high-performance synthesizer, supports SFZ/SF2 soundfonts",
    ui_title: "UI",
    theme: "Theme:",
    program_font: "Program Font:",
    font_path_placeholder: "or enter font file path...",
    font_hint: "Select a system font or specify a custom font file path. Falls back to default if invalid.",
    native_titlebar: "Use native system title bar",
    native_titlebar_hint: "When enabled, uses the native OS title bar, hiding the custom logo and window controls",
    hidpi_icon: "Enable HiDPI icon rendering (recommended)",
    hidpi_icon_hint: "Icons render at 2x resolution for retina displays. Disable to save memory and rendering overhead.",
    auto_scroll: "Auto-Scroll Settings",
    auto_scroll_fixed: "Mode 1 - Fixed indicator position:",
    auto_scroll_trigger: "Mode 2 - Page trigger offset:",
    auto_scroll_return: "Mode 2 - Page return position:",
    auto_scroll_hint: "Configure auto-scroll behavior for the playhead in the piano roll",
    interaction: "Interaction",
    selection_box_mode: "Selection Box Mode:",
    selection_box_hint: "Direct: selection box follows cursor instantly. Spring: selection box has elastic animation for a lively feel.",
    piano_roll: "Piano Roll",
    enable_256key: "Enable 256-key extended piano roll",
    enable_256key_hint: "Extends piano roll to 256 keys (0-255). Extended range (128-255) has darker tint. Requires stronger GPU.",
    pixel: "px",
    from_left: "px (from left edge)",
    from_right: "px (from right edge)",
    shortcuts_title: "Shortcuts",
    shortcuts_placeholder: "Shortcut settings content",
    about_title: "About",
    app_name: "Lumino",
    version: "Version 0.1.1-dev",
    app_description: "An efficient MIDI editor",
    high_contrast: "High Contrast",
    palette_title: "Palette",
    palette_select: "Select Palette:",
    palette_hint: "Palette files are stored in resources/palettes/ and auto-detected at compile time. Add new PNG files and rebuild to use them.",
    palette_colors_info: "Color Preview",
    palette_no_preview: "Unable to preview selected palette",
    palette_locked: "loaded, palette locked",
    editing_title: "Editing",
    editing_history_section: "Operation History",
    editing_history_total_limit: "Total history log limit:",
    editing_history_total_limit_hint: "Oldest log is evicted when exceeded (default 100, recommended 50-200)",
    editing_history_entry_limit: "Single log entry limit:",
    editing_history_entry_limit_hint: "Auto-split into new log when exceeded (default 1000, recommended 500-2000)",
    editing_merge_window: "Merge window (ms):",
    editing_merge_window_hint: "Pencil drawing only: notes placed within window merge into one undo log (0=disabled, default 300)",
    editing_intercept_section: "Edit Interception",
    editing_intercept_notification: "Show Toast on interception",
    editing_intercept_notification_hint: "Show notification when Undo/Redo/Save/Play/Export is intercepted during editing (disable for silent handling)",
    editing_tempo_max_bpm: "Tempo BPM Max:",
    editing_tempo_max_bpm_hint: "Max BPM range for the Tempo panel (default 512). Tempo points and scale lines are mapped within this range; larger values flatten the curve.",
    editing_tempo_custom_option: "Custom",
    editing_tempo_custom_title: "Custom BPM Max",
    editing_tempo_custom_placeholder: "Enter BPM max (e.g. 700)",
    ui_automation_line_thickness: "Automation line thickness:",
    ui_automation_line_thickness_hint: "Line thickness between event anchor points in the automation panel (1-10 pixels)",
    ui_monitor_refresh_interval: "Monitor refresh interval:",
    ui_monitor_refresh_interval_hint: "Refresh interval for CPU/MEM/FPS monitoring data in the bottom bar (50-2000ms, default 100ms)",
    log_retention_section: "Logging",
    log_retention_count: "Log file retention count:",
    log_retention_count_hint: "Log files are stored in logs/ under the config directory. Oldest files are auto-deleted when exceeding this limit (0 = unlimited)",
};

/// 获取设置面板翻译
pub fn get(lang: Language) -> &'static SettingsTranslations {
    match lang {
        Language::ZhCn => &ZHCN_SETTINGS,
        Language::EnUs => &ENUS_SETTINGS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_core::types::Language;

    #[test]
    fn test_settings_translations_zhcn() {
        let t = get(Language::ZhCn);
        assert_eq!(t.general, "常规");
        assert_eq!(t.confirm, "确认");
        assert_eq!(t.cancel, "取消");
        assert_eq!(t.app_name, "Lumino");
    }

    #[test]
    fn test_settings_translations_enus() {
        let t = get(Language::EnUs);
        assert_eq!(t.general, "General");
        assert_eq!(t.confirm, "Confirm");
        assert_eq!(t.cancel, "Cancel");
        assert_eq!(t.app_name, "Lumino");
    }

    #[test]
    fn test_settings_translations_not_empty() {
        for lang in [Language::ZhCn, Language::EnUs] {
            let t = get(lang);
            assert!(!t.general.is_empty());
            assert!(!t.confirm.is_empty());
            assert!(!t.audio_title.is_empty());
            assert!(!t.ui_title.is_empty());
            assert!(!t.about_title.is_empty());
        }
    }
}
