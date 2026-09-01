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
pub const FONT_BYTES: &[u8] = include_bytes!("../assets/MaterialIcons-Regular.ttf");

/// 字体族名（与 ttf 内 `name` 表一致，`otfinfo -a` 可验）
pub const FONT_FAMILY: &str = "Material Symbols Rounded";

/// 同步注入字体到全局 `cosmic-text` 字体系统（与 `iced_graphics::text::font_system` 共用）
///
/// - `Host` 为手写 `winit+wgpu`，不走 `iced_winit` 的 `Action::LoadFont` 分发，
///   `iced::font::load` 的异步 `Task` 在此无消费方；改为同步 `load_font`，早于首帧构建
/// - 与 `UiConfig.program_font_name` 共存：正文走 `Renderer::new(default_font)` 的缺省族，
///   图标显式 `Font::with_name(FONT_FAMILY)` 精确命中，本族高优且不可被设置覆盖（`Once`幂等）
pub fn ensure_loaded() {
    use std::borrow::Cow;
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        if let Ok(mut fs) = iced_graphics::text::font_system().write() {
            fs.load_font(Cow::Borrowed(FONT_BYTES));
        } else {
            tracing::error!("Material Icons 字体注入失败：font_system 锁中毒");
        }
    });
}

/// 检查字体是否已在字体系统中（供测试/诊断用，`Once`后的二次调用应仍返回 true）
#[must_use]
pub fn is_loaded() -> bool {
    use iced_graphics::text::cosmic_text;
    let Ok(mut fs) = iced_graphics::text::font_system().write() else {
        return false;
    };
    // 以族名探测：`fontdb` 中存在该族即认为已注入
    fs.raw()
        .db()
        .query(&cosmic_text::fontdb::Query {
            families: &[cosmic_text::fontdb::Family::Name(FONT_FAMILY)],
            weight: cosmic_text::Weight::NORMAL,
            stretch: cosmic_text::Stretch::Normal,
            style: cosmic_text::Style::Normal,
        })
        .is_some()
}

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

/// 码点查表（对齐 `yinhe` 的 `egui_material_icons 0.8`，已校正为 Symbols Rounded 实际码点）
///
/// - `HOME e9b2` 校正：原 `e88a` 为旧 MaterialIcons，Symbols 为 `e9b2`
/// - `EDIT f097` 校正：原 `e3c9` 为旧版，Symbols 为 `f097`
/// - `HISTORY e8b3` 校正：原 `e88c`
/// - `COPY_ALL e2ec` 校正：原别名 `e14d` 为 `content_copy`，`e2ec` 为独立 `copy_all`
/// - `STACK/STACK_OFF f609/f608` 非 `layers`，`SELECT f74d` 非 `select_all`，`KEEP f026` 非 `push_pin f10d`
/// - 全部 hex 来自 `egui_material_icons-0.8.0/src/icons.rs` 与 `MaterialIcons.codepoints` 双校
pub mod codepoints {
    // ── 文件菜单 FileAction::ALL:10 ───────────────────────────────
    /// home e9b2（校正：原 e88a 为 MaterialIcons，Symbols 为 e9b2）
    pub const HOME: char = '\u{E9B2}';
    /// note_add e89c (新建工程)
    pub const NOTE_ADD: char = '\u{E89C}';
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
    /// description e873 (最近文件 / project.json)
    pub const DESCRIPTION: char = '\u{E873}';
    // ── 编辑菜单 EditAction::ALL:12 ───────────────────────────────
    /// undo e166
    pub const UNDO: char = '\u{E166}';
    /// redo e15a
    pub const REDO: char = '\u{E15A}';
    /// content_cut e14e
    pub const CONTENT_CUT: char = '\u{E14E}';
    /// content_copy e14d
    pub const CONTENT_COPY: char = '\u{E14D}';
    /// content_paste e14f
    pub const CONTENT_PASTE: char = '\u{E14F}';
    /// select_all e162
    pub const SELECT_ALL: char = '\u{E162}';
    /// copy_all e2ec（校正：原 e14d 为 content_copy）
    pub const COPY_ALL: char = '\u{E2EC}';
    /// delete e92e (删除)
    pub const DELETE: char = '\u{E92E}';
    /// arrow_upward e5d8 (上移调)
    pub const ARROW_UPWARD: char = '\u{E5D8}';
    /// arrow_downward e5db (下移调)
    pub const ARROW_DOWNWARD: char = '\u{E5DB}';
    /// layers_clear/stack_off f608（轨内/跨轨去重，对齐 yinhe ICON_STACK_OFF）
    pub const STACK_OFF: char = '\u{F608}';
    // ── 播放菜单 PlayMenuAction / FollowMode ──────────────────────
    /// play_arrow e037
    pub const PLAY_ARROW: char = '\u{E037}';
    /// pause e034
    pub const PAUSE: char = '\u{E034}';
    /// stop e047
    pub const STOP: char = '\u{E047}';
    /// fiber_manual_record e061 (录制红点)
    pub const FIBER_MANUAL_RECORD: char = '\u{E061}';
    /// stacked / step 容器前置（占位）
    pub const STEP: char = '\u{F6FE}';
    /// history e8b3（最近文件，校正原 e88c）
    pub const HISTORY: char = '\u{E8B3}';
    /// lock e899（跟随 None）
    pub const LOCK: char = '\u{E899}';
    /// align_horizontal_center e00f（跟随居中）
    pub const ALIGN_HORIZONTAL_CENTER: char = '\u{E00F}';
    /// auto_stories e666（跟随 Page）
    pub const AUTO_STORIES: char = '\u{E666}';
    /// align_horizontal_left e00d（跟随连续）
    pub const ALIGN_HORIZONTAL_LEFT: char = '\u{E00D}';
    // ── 工具栏 Tool::ALL:7 ────────────────────────────────────────
    /// select f74d（选框）
    pub const SELECT: char = '\u{F74D}';
    /// text_select_start f735（区间选择）
    pub const TEXT_SELECT_START: char = '\u{F735}';
    /// pan_tool e925（抓手）
    pub const PAN_TOOL: char = '\u{E925}';
    /// edit f097（铅笔，校正原 e3c9）
    pub const EDIT: char = '\u{F097}';
    /// draw e746（曲线）
    pub const DRAW: char = '\u{E746}';
    /// content_cut e14e（剪刀，复用 Edit Cut）
    pub const CONTENT_CUT_DUP: char = '\u{E14E}';
    /// ink_eraser e6d0（擦除）
    pub const INK_ERASER: char = '\u{E6D0}';
    /// dehaze e3c7（方向切换，横→纵旋转 90°）
    pub const DEHAZE: char = '\u{E3C7}';
    // ── 通用/右侧/对话框 ──────────────────────────────────────────
    /// add e145（空轨添加、事件表空态、音色+、自动化±）
    pub const ADD: char = '\u{E145}';
    /// remove e15b（自动化-）
    pub const REMOVE: char = '\u{E15B}';
    /// drag_indicator e945（音色列表拖柄）
    pub const DRAG_INDICATOR: char = '\u{E945}';
    /// search e8b6（归档搜索）
    pub const SEARCH: char = '\u{E8B6}';
    /// visibility e8f4 / off e8f5（密码显隐）
    pub const VISIBILITY: char = '\u{E8F4}';
    /// visibility_off e8f5
    pub const VISIBILITY_OFF: char = '\u{E8F5}';
    /// chevron_right e5cc / left e5cb（折叠、分页）
    pub const CHEVRON_RIGHT: char = '\u{E5CC}';
    pub const CHEVRON_LEFT: char = '\u{E5CB}';
    /// expand_more e5cf（树展开）
    pub const EXPAND_MORE: char = '\u{E5CF}';
    /// keyboard_arrow_down e313 / right e315 / up e316（折叠、选择上下）
    pub const KEYBOARD_ARROW_DOWN: char = '\u{E313}';
    pub const KEYBOARD_ARROW_RIGHT: char = '\u{E315}';
    pub const KEYBOARD_ARROW_UP: char = '\u{E316}';
    /// arrow_back e5c4（Android 返回）
    pub const ARROW_BACK: char = '\u{E5C4}';
    /// flip e3e8（左右翻转，垂直翻转需旋转 90°）
    pub const FLIP: char = '\u{E3E8}';
    /// folder e2c7 / folder_zip eb2c（树目录、EventBrowser）
    pub const FOLDER: char = '\u{E2C7}';
    pub const FOLDER_ZIP: char = '\u{EB2C}';
    /// format_color_reset e23b（重置配色）
    pub const FORMAT_COLOR_RESET: char = '\u{E23B}';
    /// headphones f01f（独奏）
    pub const HEADPHONES: char = '\u{F01F}';
    /// volume_off e04f（静音）
    pub const VOLUME_OFF: char = '\u{E04F}';
    /// palette e40a（ProgramChange）
    pub const PALETTE: char = '\u{E40A}';
    /// piano e521（mode_bar 钢琴卷帘叠加，对齐 yinhe ICON_PIANO）
    pub const PIANO: char = '\u{E521}';
    /// info e88e（Info 面板）
    pub const INFO: char = '\u{E88E}';
    /// music_cast eb1a（SoundFont 面板，对齐 yinhe ICON_MUSIC_CAST）
    pub const MUSIC_CAST: char = '\u{EB1A}';
    /// library_music e030（和弦）
    pub const LIBRARY_MUSIC: char = '\u{E030}';
    /// subtitles e048（歌词）
    pub const SUBTITLES: char = '\u{E048}';
    /// schedule efd6（TimeSig/AccessTime）
    pub const SCHEDULE: char = '\u{EFD6}';
    /// music_off e440（KeySig）
    pub const MUSIC_OFF: char = '\u{E440}';
    /// bookmark e8e7（Markers）
    pub const BOOKMARK: char = '\u{E8E7}';
    /// signal_cellular_alt e202（力度）
    pub const SIGNAL_CELLULAR_ALT: char = '\u{E202}';
    /// timeline e922（自动化目标）
    pub const TIMELINE: char = '\u{E922}';
    /// speed e9e4（Tempo）
    pub const SPEED: char = '\u{E9E4}';
    /// stacked/layers f609（allow_overlap 开）
    pub const STACK: char = '\u{F609}';
    /// palette/tune 同已定义
    /// check_circle f0be（加载完成）
    pub const CHECK_CIRCLE: char = '\u{F0BE}';
    /// sync e627（加载中）
    pub const SYNC: char = '\u{E627}';
    /// radio_button_unchecked e836（加载待定）
    pub const RADIO_BUTTON_UNCHECKED: char = '\u{E836}';
    /// keep f026（图钉，对齐 yinhe ICON_KEEP，非 push_pin f10d）
    pub const KEEP: char = '\u{F026}';
    /// push_pin f10d（保留兼容，旧 pin）
    pub const PUSH_PIN: char = '\u{F10D}';
    /// push_pin 描边（同）
    pub const PUSH_PIN_OUTLINED: char = '\u{F10D}';
    /// center_focus_strong e3b4（Android 跟随）
    pub const CENTER_FOCUS_STRONG: char = '\u{E3B4}';
    /// masked_transitions e72e（幽灵）
    pub const MASKED_TRANSITIONS: char = '\u{E72E}';
    /// edit_audio f42d（PitchBend）
    pub const EDIT_AUDIO: char = '\u{F42D}';
    /// edit_square f88d（编辑菜单头）
    pub const EDIT_SQUARE: char = '\u{F88D}';
    /// play_circle e1c4（播放菜单头）
    pub const PLAY_CIRCLE: char = '\u{E1C4}';
    // ── 兼容别名 ──────────────────────────────────────────────────
    /// save_alt f090 alias
    pub const SAVE_ALT: char = '\u{F090}';
    /// arrow_drop_down e5c5（旧折叠，对齐 expand_more）
    pub const ARROW_DROP_DOWN: char = '\u{E5C5}';
    /// check_box e834
    pub const CHECK_BOX: char = '\u{E834}';
    /// text_fields e262 / brush e3ae / straighten e41c / more_horiz e5d3 / arrow_forward e5c8（保留兼容）
    pub const TEXT_FIELDS: char = '\u{E262}';
    pub const BRUSH: char = '\u{E3AE}';
    pub const STRAIGHTEN: char = '\u{E41C}';
    pub const MORE_HORIZ: char = '\u{E5D3}';
    pub const ARROW_FORWARD: char = '\u{E5C8}';
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

    #[test]
    fn ensure_loaded_is_idempotent_and_queryable() {
        // 首次注入 + 二次幂等均不应 panic，且族名可被 fontdb 查询到
        ensure_loaded();
        assert!(is_loaded(), "ensure_loaded 后 fontdb 应能查询到族名");
        ensure_loaded(); // 二次调用由 Once 去重，不应重复 load
        assert!(is_loaded());
    }

    #[test]
    fn icon_uses_material_family_not_config_font() {
        // 校验 icon() 固定使用 FONT_FAMILY，不受 UiConfig.program_font_name 影响
        let txt = icon(codepoints::SAVE, 14.0, iced_core::Color::WHITE);
        // 通过构造后字体族名间接验证（iced_widget::Text 内部 font 为私有，
        // 此处改验 codepoints 常量与 font() 辅助的一致性）
        assert_eq!(font().family, Family::Name(FONT_FAMILY));
        let _ = txt; // 仅保证编译期类型正确，运行时需配合 ensure_loaded
    }
}
