//! 扩展功能模块
//!
//! 提供多语言支持和调色板管理等扩展功能。

/// 图片转 MIDI 的颜色后处理（DRAW-030）
pub mod image_adjust;
pub mod material;
pub mod palette;

pub use image_adjust::{Adjust, adjust_bgra};

pub use material::{EmbeddedMaterial, embedded_materials};

pub use palette::{
    EmbeddedPalette, PALETTE_MANAGER, Palette, PaletteColor, PaletteManager, current_palette_idx,
    current_palette_name, current_track_color, current_track_color_f32, is_palette_locked,
    lock_palette, onion_track_color, onion_track_color_f32, reset_current_palette,
    set_current_palette_by_name, unlock_palette,
};
