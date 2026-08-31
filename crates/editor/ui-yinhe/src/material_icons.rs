//! Material Icons 字体支持 — iced 0.14 专用（对齐 yinhe egui_material_icons 0.8）
//!
//! yinhe 原 `egui_material_icons::icons::*` 在 iced 侧通过 PUA 私有区 codepoint + `Font::with_name` 复刻。
//! - 字体文件：`crates/editor/ui-yinhe/assets/MaterialIcons-Regular.ttf`（从 `egui_material_icons-0.8.0` 复制）
//! - 注册：`Host::new` 时 `font::load` 或 `application().font()`，此处提供 `FONT_BYTES` 与 `FONT` 常量
//! - 码点：`assets/MaterialIcons.codepoints`（`name hex`）已随 ttf 同步，需查表得 char
//!
//! 坑：
//! - PUA 编码 `\uE000` 起，非普通 Unicode，写字符前必查表
//! - `Font::with_name("Material Symbols Rounded")` 必须与 ttf 内 `name` 表一致（此处为 `Material Symbols Rounded`）
//! - 0.14 `font::load` 为 async，需 `Task::perform` 包裹

use iced_core::Font;
use iced_core::font::Family;

/// Material Icons 字体字节（编译时内嵌，避免运行时文件不存在）
pub const FONT_BYTES: &[u8] =
    include_bytes!("../assets/MaterialIcons-Regular.ttf");

/// 字体族名（与 ttf 内 `name` 表一致，`otfinfo -a` 可验）
pub const FONT_FAMILY: &str = "Material Symbols Rounded";

/// iced Font 句柄（`Font::with_name` 必须一字不差）
pub fn font() -> Font {
    Font {
        family: Family::Name(FONT_FAMILY),
        ..Font::default()
    }
}

/// 便捷：构造 Material Icons 文本（PUA char + 字体 + 尺寸 + 颜色）
///
/// ```ignore
/// use lumino_ui_yinhe::material_icons::{icon, font};
/// let home = icon('\u{E88A}', 24.0, Color::WHITE);
/// ```
pub fn icon<'a>(
    codepoint: char,
    size: f32,
    color: iced_core::Color,
) -> iced_widget::Text<'a, iced_core::Theme, iced_core::Renderer> {
    use iced_widget::text;
    text(codepoint.to_string())
        .font(font())
        .size(size)
        .color(color)
}

/// 码点查表（常用 yinhe 图标，来自 `MaterialIcons.codepoints`）
pub mod codepoints {
    /// home e88a
    pub const HOME: char = '\u{E88A}';
    /// play_arrow e037
    pub const PLAY_ARROW: char = '\u{E037}';
    /// pause e034
    pub const PAUSE: char = '\u{E034}';
    /// stop e047
    pub const STOP: char = '\u{E047}';
    /// fiber_manual_record e061 (录制红点)
    pub const FIBER_MANUAL_RECORD: char = '\u{E061}';
    /// save e161
    pub const SAVE: char = '\u{E161}';
    /// folder_open e2c8
    pub const FOLDER_OPEN: char = '\u{E2C8}';
    /// edit e3c9
    pub const EDIT: char = '\u{E3C9}';
    /// content_cut e14e
    pub const CONTENT_CUT: char = '\u{E14E}';
    /// content_copy e14d
    pub const CONTENT_COPY: char = '\u{E14D}';
    /// content_paste e14f
    pub const CONTENT_PASTE: char = '\u{E14F}';
    /// delete e872
    pub const DELETE: char = '\u{E872}';
    /// undo e166
    pub const UNDO: char = '\u{E166}';
    /// redo e15a
    pub const REDO: char = '\u{E15A}';
    /// select_all e162
    pub const SELECT_ALL: char = '\u{E162}';
    /// text_fields e262
    pub const TEXT_FIELDS: char = '\u{E262}';
    /// brush e3ae
    pub const BRUSH: char = '\u{E3AE}';
    /// draw e3ae (alias)
    pub const DRAW: char = '\u{E3AE}';
    /// pan_tool e925
    pub const PAN_TOOL: char = '\u{E925}';
    /// straighten e41c (尺子/量化)
    pub const STRAIGHTEN: char = '\u{E41C}';
    /// tune e429 (调音/工具)
    pub const TUNE: char = '\u{E429}';
    /// settings e8b8
    pub const SETTINGS: char = '\u{E8B8}';
    /// more_horiz e5d3
    pub const MORE_HORIZ: char = '\u{E5D3}';
    /// arrow_drop_down e5c5
    pub const ARROW_DROP_DOWN: char = '\u{E5C5}';
    /// check_box e834
    pub const CHECK_BOX: char = '\u{E834}';
    /// push_pin e55f (图钉)
    pub const PUSH_PIN: char = '\u{E55F}';
    /// push_pin 描边 e55f 同
    pub const PUSH_PIN_OUTLINED: char = '\u{E55F}';
}

/// 加载任务（0.14 async，需 Task::perform）
pub fn load_task() -> iced::Task<Result<(), iced::font::Error>> {
    iced::font::load(FONT_BYTES).map(|r| r.map(|_| ()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_bytes_non_empty() {
        assert!(!FONT_BYTES.is_empty());
        assert!(FONT_BYTES.len() > 100_000);
    }

    #[test]
    fn font_name_matches() {
        let f = font();
        assert_eq!(f.family, Family::Name(FONT_FAMILY));
    }

    #[test]
    fn codepoints_are_pua() {
        // Material Icons 全在 E000-F8FF 私有区
        for &cp in &[codepoints::HOME, codepoints::PLAY_ARROW, codepoints::SAVE] {
            assert!((0xE000..=0xF8FF).contains(&(cp as u32)));
        }
    }
}
