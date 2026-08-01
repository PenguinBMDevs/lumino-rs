//! 扩展功能模块
//!
//! 提供多语言支持和调色板管理等扩展功能。

pub mod i18n;
pub mod palette;

pub use i18n::{
    Language, MainTranslations, SettingsTranslations, dot_type_name, eraser_behavior_name,
    main_translations, note_precision_name, selection_box_mode_name, settings_translations,
    synth_backend_name, track_add_behavior_name,
};
pub use palette::{
    EmbeddedPalette, PALETTE_MANAGER, Palette, PaletteColor, PaletteManager, current_palette_name,
    current_track_color, current_track_color_f32, is_palette_locked, lock_palette,
    onion_track_color, onion_track_color_f32, reset_current_palette, set_current_palette_by_name,
    unlock_palette,
};
