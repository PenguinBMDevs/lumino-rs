//! 设置面板的阻塞式文件选择器（音色库 / 程序字体）
//!
//! 从 `panel.rs` 的 `update` 匹配臂拆出：这两处是面板里唯一的**阻塞式文件对话框**
//! 调用（打开系统模态窗口），与「事件 → 状态」的分发逻辑没有耦合，独立成文件后
//! `panel.rs` 的匹配臂各只剩一行委托调用。
//!
//! 逐字搬迁，语义零变化——包括「选音色库只改路径」与
//! 「选字体文件时清空系统字体名（UI-001 互斥）」两条既有约定。

use lumino_core::tr;

use crate::SettingsPanel;

/// 浏览选择音色库文件（SF2 / SFZ），写入 `synth.soundfont_path`。
///
/// 取消选择时保持原值不变。
pub(super) fn browse_soundfont(panel: &mut SettingsPanel) {
    if let Some(path) = lumino_platform::dialog::FileDialog::new()
        .add_filter(tr!("音色库文件"), &["sf2", "sfz"])
        .add_filter(tr!("SF2 文件"), &["sf2"])
        .add_filter(tr!("SFZ 文件"), &["sfz"])
        .add_filter(tr!("所有文件"), &["*"])
        .pick_file()
    {
        panel.synth.soundfont_path = path.to_string_lossy().into_owned();
    }
}

/// 浏览选择程序字体文件，写入 `editing.program_font_path`。
///
/// UI-001：与系统字体名互斥（浏览选择文件 → 清空字体名）。
pub(super) fn browse_program_font(panel: &mut SettingsPanel) {
    if let Some(path) = lumino_platform::dialog::FileDialog::new()
        .add_filter(tr!("字体文件"), &["ttf", "otf", "ttc", "woff", "woff2"])
        .add_filter(tr!("TrueType 字体"), &["ttf"])
        .add_filter(tr!("OpenType 字体"), &["otf"])
        .add_filter(tr!("所有文件"), &["*"])
        .pick_file()
    {
        panel.editing.program_font_path = path.to_string_lossy().into_owned();
        // UI-001：与系统字体名互斥（浏览选择文件 → 清空字体名）
        panel.editing.program_font_name.clear();
    }
}
