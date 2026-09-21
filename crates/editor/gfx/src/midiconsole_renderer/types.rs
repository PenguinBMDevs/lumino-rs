//! MidiConsole GPU 渲染器 — CPU 侧颜色打包工具

/// 把 `CellGpu` 单元数组打包为 0xRRGGBB 所需的小工具（供调用方复用）。
pub fn pack_rgb(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}
