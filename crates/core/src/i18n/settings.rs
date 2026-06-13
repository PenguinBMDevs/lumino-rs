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
    pub textured_keyboard: &'static str,
    pub textured_keyboard_hint: &'static str,
    pub pixel: &'static str,
    pub from_left: &'static str,
    pub from_right: &'static str,

    // ── 快捷键页面 ──
    pub shortcuts_title: &'static str,
    pub shortcuts_placeholder: &'static str,

    // ── 关于页面 ──
    pub about_title: &'static str,
    pub app_name: &'static str,
    pub version: &'static str,
    pub app_description: &'static str,
    /// 高对比度主题显示名称
    pub high_contrast: &'static str,
}

static ZHCN_SETTINGS: SettingsTranslations = SettingsTranslations {
    general: "常规",
    audio: "音频",
    ui: "界面",
    shortcuts: "快捷键",
    about: "关于",
    confirm: "确认",
    cancel: "取消",
    ok: "确定",
    or: "或",
    browse: "浏览...",
    general_title: "常规",
    eraser_behavior: "橡皮擦行为:",
    eraser_default_hint: "默认: Shift+拖动框选删除，点击删除单个",
    eraser_direct_hint: "直接框选: 拖动框选删除，Shift+点击删除单个",
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
    textured_keyboard: "使用钢琴仿真键盘（推荐）",
    textured_keyboard_hint: "开启后使用真实钢琴贴图渲染键盘，视觉效果更佳。关闭则使用传统纯色键盘。",
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
};

static ENUS_SETTINGS: SettingsTranslations = SettingsTranslations {
    general: "General",
    audio: "Audio",
    ui: "UI",
    shortcuts: "Shortcuts",
    about: "About",
    confirm: "Confirm",
    cancel: "Cancel",
    ok: "OK",
    or: "or",
    browse: "Browse...",
    general_title: "General",
    eraser_behavior: "Eraser Behavior:",
    eraser_default_hint: "Default: Shift+drag to box-delete, click to delete single",
    eraser_direct_hint: "Direct Select: drag to box-delete, Shift+click to delete single",
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
    textured_keyboard: "Use textured piano keyboard (recommended)",
    textured_keyboard_hint: "Renders keyboard with realistic piano texture. Disable for traditional solid-color keyboard.",
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
    use crate::i18n::Language;

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
