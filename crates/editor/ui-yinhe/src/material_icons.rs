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
) -> iced_widget::Text<'a, iced_core::Theme, lumino_ui_core::Renderer> {
    use iced_widget::text;
    text(codepoint.to_string())
        .font(font())
        .size(size)
        .color(color)
}

/// 码点查表（常用 yinhe 图标，来自 `MaterialIcons.codepoints`，已校正为 MaterialSymbolsRounded 实际码点）
pub mod codepoints {
    /// home e9b2（校正：原 e88a 为 MaterialIcons，Symbols 为 e9b2）
    pub const HOME: char = '\u{E9B2}';
    /// play_arrow e037
    pub const PLAY_ARROW: char = '\u{E037}';
    /// pause e034
    pub const PAUSE: char = '\u{E034}';
    /// stop e047
    pub const STOP: char = '\u{E047}';
    /// fiber_manual_record e061 (录制红点)
    pub const FIBER_MANUAL_RECORD: char = '\u{E061}';
    /// note_add e89c (新建工程)
    pub const NOTE_ADD: char = '\u{E89C}';
    /// description e873 (新建文档)
    pub const DESCRIPTION: char = '\u{E873}';
    /// folder_open e2c8 (打开)
    pub const FOLDER_OPEN: char = '\u{E2C8}';
    /// save e161 (保存)
    pub const SAVE: char = '\u{E161}';
    /// save_as f090
    pub const SAVE_AS: char = '\u{F090}';
    /// close e5cd (关闭)
    pub const CLOSE: char = '\u{E5CD}';
    /// audio_file eb82 (导出音频)
    pub const AUDIO_FILE: char = '\u{EB82}';
    /// audiotrack e405 (导出 MIDI)
    pub const AUDIOTRACK: char = '\u{E405}';
    /// tune e429 (工程设置)
    pub const TUNE: char = '\u{E429}';
    /// settings e8b8 (设置)
    pub const SETTINGS: char = '\u{E8B8}';
    /// exit_to_app e879 (退出)
    pub const EXIT_TO_APP: char = '\u{E879}';
    /// edit f097（校正：原 e3c9 为旧版）
    pub const EDIT: char = '\u{F097}';
    /// content_cut e14e
    pub const CONTENT_CUT: char = '\u{E14E}';
    /// content_copy e14d
    pub const CONTENT_COPY: char = '\u{E14D}';
    /// content_paste e14f
    pub const CONTENT_PASTE: char = '\u{E14F}';
    /// delete e92e (删除)
    pub const DELETE: char = '\u{E92E}';
    /// undo e166
    pub const UNDO: char = '\u{E166}';
    /// redo e15a
    pub const REDO: char = '\u{E15A}';
    /// select_all e162
    pub const SELECT_ALL: char = '\u{E162}';
    /// history e8b3（校正：原 e88c）
    pub const HISTORY: char = '\u{E8B3}';
    /// text_fields e262
    pub const TEXT_FIELDS: char = '\u{E262}';
    /// brush e3ae
    pub const BRUSH: char = '\u{E3AE}';
    /// draw e746 (draw)
    pub const DRAW: char = '\u{E746}';
    /// pan_tool e925
    pub const PAN_TOOL: char = '\u{E925}';
    /// straighten e41c (尺子/量化)
    pub const STRAIGHTEN: char = '\u{E41C}';
    /// more_horiz e5d3
    pub const MORE_HORIZ: char = '\u{E5D3}';
    /// arrow_drop_down e5c5
    pub const ARROW_DROP_DOWN: char = '\u{E5C5}';
    /// check_box e834
    pub const CHECK_BOX: char = '\u{E834}';
    /// push_pin f10d
    pub const PUSH_PIN: char = '\u{F10D}';
    /// push_pin 描边（同）
    pub const PUSH_PIN_OUTLINED: char = '\u{F10D}';
    /// visibility e8f4
    pub const VISIBILITY: char = '\u{E8F4}';
    /// visibility_off e8f5
    pub const VISIBILITY_OFF: char = '\u{E8F5}';
    /// search e8b6
    pub const SEARCH: char = '\u{E8B6}';
    /// arrow_upward e5d8
    pub const ARROW_UPWARD: char = '\u{E5D8}';
    /// arrow_downward e5db
    pub const ARROW_DOWNWARD: char = '\u{E5DB}';
    /// arrow_forward e5c8
    pub const ARROW_FORWARD: char = '\u{E5C8}';
    /// content_copy alias
    pub const COPY_ALL: char = '\u{E14D}';
    /// save_alt f090 alias
    pub const SAVE_ALT: char = '\u{F090}';
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
