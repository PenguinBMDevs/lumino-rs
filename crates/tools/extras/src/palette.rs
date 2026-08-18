//! 颜色贴图（调色板）系统
//!
//! 从 `resources/palettes/` 目录下的 PNG 图片中读取调色板颜色。
//! 图片格式：一行像素或多行像素（16x1、32x1、16x8、32x8 等），
//! 每个像素代表一个颜色。支持 RGBA、RGB、Indexed 三种 PNG 颜色类型。
//!
//! 设计参考：Zenith-MIDI 的 `NoteColorPalettePick` 系统。
//! - 所有调色板文件在编译时通过 `build.rs` 自动检测并嵌入
//! - 运行时不依赖文件系统路径
//!
//! 该模块已拆分为以下子模块：
//! - `manager`: 调色板管理器（PALETTE_MANAGER 全局单例）
//! - `png`: PNG 解码（decode_palette_png）
//! - `state`: 全局当前调色板状态与快捷取色函数

mod manager;
mod png;
mod state;

pub use manager::{PALETTE_MANAGER, PaletteManager};
pub use png::PngDecodeError;
pub use state::{
    current_palette_idx, current_palette_name, current_track_color, current_track_color_f32,
    is_palette_locked, lock_palette, onion_track_color, onion_track_color_f32,
    reset_current_palette, set_current_palette_by_name, unlock_palette,
};

/// 调色板颜色（RGBA，各分量 0-255）
pub type PaletteColor = [u8; 4];

/// 编译时嵌入的调色板数据
#[derive(Debug, Clone)]
pub struct EmbeddedPalette {
    /// 显示名称（文件名不含扩展名）
    pub name: &'static str,
    /// PNG 原始字节
    pub data: &'static [u8],
}

/// 解析后的调色板
#[derive(Debug, Clone)]
pub struct Palette {
    /// 显示名称
    pub name: &'static str,
    /// 颜色列表
    pub colors: Vec<PaletteColor>,
}

/// 默认备用调色板（硬编码，当嵌入数据为空时使用）
pub(crate) const FALLBACK_PALETTE: [PaletteColor; 12] = [
    [200, 80, 80, 255],
    [80, 200, 120, 255],
    [80, 120, 220, 255],
    [220, 200, 80, 255],
    [200, 100, 200, 255],
    [80, 200, 200, 255],
    [240, 150, 80, 255],
    [180, 180, 180, 255],
    [230, 100, 100, 255],
    [100, 180, 100, 255],
    [100, 100, 200, 255],
    [200, 180, 100, 255],
];

impl Palette {
    /// 获取指定索引的颜色（循环取色）
    pub fn track_color(&self, track_idx: usize) -> PaletteColor {
        if self.colors.is_empty() {
            return FALLBACK_PALETTE[track_idx % FALLBACK_PALETTE.len()];
        }
        self.colors[track_idx % self.colors.len()]
    }

    /// 获取颜色作为 `[f32; 4]`（归一化到 0.0-1.0）
    pub fn track_color_f32(&self, track_idx: usize) -> [f32; 4] {
        let color = self.track_color(track_idx);
        [
            color[0] as f32 / 255.0,
            color[1] as f32 / 255.0,
            color[2] as f32 / 255.0,
            color[3] as f32 / 255.0,
        ]
    }
}

#[cfg(test)]
mod tests;
