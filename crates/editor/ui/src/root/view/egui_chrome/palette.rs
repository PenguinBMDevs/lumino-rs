//! 主题映射：把当前 iced 主题的调色板转成 egui 语义色板。
//!
//! egui 外壳（标题栏 / 工具栏 / 侧栏 / 状态栏 / 对话框）是不透明 band，颜色必须
//! 与 iced 当前主题一致，否则默认 `Light` 配置下会出现「深色外壳 + 浅色内容」
//! 的双主题叠层。这里只做数据转换，`lumino-ui-egui` 本身不依赖 iced。

use lumino_ui_egui::theme::{Palette, PaletteSeed};

use crate::root::Root;

use super::color32;

impl Root {
    /// 当前窗口主题对应的 egui 色板。
    ///
    /// 色值来源与 iced 样式完全同源（`Theme::extended_palette()`），因此任何
    /// 内置主题（Light / Dark / Solarized / Catppuccin / Tokyo Night …）下
    /// egui 外壳与 iced 画面都保持同一套配色。
    pub(crate) fn egui_palette(&self) -> Palette {
        let theme = self.theme();
        let ext = theme.extended_palette();
        Palette::from_seed(&PaletteSeed {
            is_dark: ext.is_dark,
            base: color32(ext.background.base.color),
            weakest: color32(ext.background.weakest.color),
            weaker: color32(ext.background.weaker.color),
            weak: color32(ext.background.weak.color),
            neutral: color32(ext.background.neutral.color),
            strong: color32(ext.background.strong.color),
            text: color32(ext.background.base.text),
            text_on_weak: color32(ext.background.weak.text),
            primary: color32(ext.primary.base.color),
            primary_strong: color32(ext.primary.strong.color),
            primary_weak: color32(ext.primary.weak.color),
            success: color32(ext.success.base.color),
            warning: color32(ext.warning.base.color),
            danger: color32(ext.danger.base.color),
        })
    }
}
