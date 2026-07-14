//! 颜色贴图（调色板）系统
//!
//! 从 `resources/palettes/` 目录下的 PNG 图片中读取调色板颜色。
//! 图片格式：一行像素或多行像素（16x1、32x1、16x8、32x8 等），
//! 每个像素代表一个颜色。支持 RGBA、RGB、Indexed 三种 PNG 颜色类型。
//!
//! 设计参考：Zenith-MIDI 的 `NoteColorPalettePick` 系统。
//! - 所有调色板文件在编译时通过 `build.rs` 自动检测并嵌入
//! - 运行时不依赖文件系统路径

use std::sync::LazyLock;

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
const FALLBACK_PALETTE: [PaletteColor; 12] = [
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

// ─── 编译时嵌入的数据 ─────────────────────────────────────────────────────────
// 由 build.rs 自动生成，扫描 `resources/palettes/` 目录下的所有 PNG 文件

/// 所有嵌入的调色板原始数据（编译时自动检测）
///
/// 由 build.rs 在编译时根据 resources/palettes/ 目录生成
fn embedded_palettes() -> &'static [EmbeddedPalette] {
    const DATA: &[EmbeddedPalette] = &include!(concat!(env!("OUT_DIR"), "/palettes.rs"));
    DATA
}

// ─── 调色板管理器 ────────────────────────────────────────────────────────────

/// 调色板管理器（全局单例）
///
/// 管理所有编译时嵌入的调色板，提供查询和获取功能。
pub static PALETTE_MANAGER: LazyLock<PaletteManager> = LazyLock::new(PaletteManager::new);

/// 调色板管理器
#[derive(Debug)]
pub struct PaletteManager {
    /// 所有已解析的调色板
    palettes: Vec<Palette>,
    /// 调色板名称列表
    names: Vec<&'static str>,
}

impl PaletteManager {
    /// 创建新的调色板管理器（解析所有嵌入数据）
    fn new() -> Self {
        let mut palettes: Vec<Palette> = embedded_palettes()
            .iter()
            .filter_map(|ep| match decode_palette_png(ep.data) {
                Ok(colors) => Some(Palette {
                    name: ep.name,
                    colors,
                }),
                Err(e) => {
                    tracing::warn!("[Palette] 解析调色板失败 '{}': {}", ep.name, e);
                    None
                }
            })
            .collect();

        // 如果没有成功解析任何调色板，使用硬编码备用
        if palettes.is_empty() {
            tracing::warn!("[Palette] 没有可用的调色板文件，使用硬编码备用");
            palettes.push(Palette {
                name: "Default",
                colors: FALLBACK_PALETTE.to_vec(),
            });
        }

        // 将名为 "Random" 的调色板冒泡到第一个位置，作为默认调色板
        if let Some(random_idx) = palettes.iter().position(|p| p.name == "Random")
            && random_idx != 0
        {
            let random = palettes.remove(random_idx);
            palettes.insert(0, random);
        }

        let names = palettes.iter().map(|p| p.name).collect();
        Self { palettes, names }
    }

    /// 获取所有调色板名称
    pub fn names(&self) -> &[&'static str] {
        &self.names
    }

    /// 获取所有调色板（引用）
    pub fn palettes(&self) -> &[Palette] {
        &self.palettes
    }

    /// 按名称获取调色板
    pub fn get(&self, name: &str) -> Option<&Palette> {
        self.palettes.iter().find(|p| p.name == name)
    }

    /// 获取默认调色板（第一个）
    pub fn default(&self) -> &Palette {
        &self.palettes[0]
    }

    /// 获取有效调色板名称（如果不存在则返回默认名称）
    pub fn resolve_name(&self, name: &str) -> &'static str {
        for &n in &self.names {
            if n == name {
                return n;
            }
        }
        self.names[0]
    }
}

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
        let c = self.track_color(track_idx);
        [
            c[0] as f32 / 255.0,
            c[1] as f32 / 255.0,
            c[2] as f32 / 255.0,
            c[3] as f32 / 255.0,
        ]
    }
}

// ─── PNG 解码 ────────────────────────────────────────────────────────────────

/// PNG 解压错误
#[derive(Debug)]
pub struct PngDecodeError {
    msg: String,
}

impl std::fmt::Display for PngDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PNG 解码错误: {}", self.msg)
    }
}

impl std::error::Error for PngDecodeError {}

/// 解码调色板 PNG 图片
///
/// 支持的格式：
/// - 颜色类型：RGB (2)、RGBA (6)、Indexed (3)
/// - 宽度：任意（推荐 16 或 32 像素）
/// - 高度：任意（推荐 1 或 8 行）
///
/// 返回所有像素的 RGBA 颜色列表。
fn decode_palette_png(data: &[u8]) -> Result<Vec<PaletteColor>, PngDecodeError> {
    use png::ColorType;
    let decoder = png::Decoder::new(data);
    let mut reader = decoder.read_info().map_err(|e| PngDecodeError {
        msg: format!("无法读取 PNG: {}", e),
    })?;

    // Clone the info data to avoid borrow issues with reader
    let width = reader.info().width as usize;
    let height = reader.info().height as usize;
    let color_type = reader.info().color_type;
    let bit_depth = reader.info().bit_depth;
    let palette_data = reader.info().palette.clone();
    let trns_data = reader.info().trns.clone();

    // 分配输出缓冲
    let output_size = reader.output_buffer_size();
    let mut buf = vec![0u8; output_size];
    let _ = reader.next_frame(&mut buf).map_err(|e| PngDecodeError {
        msg: format!("无法解码帧: {}", e),
    })?;

    let colors = match (color_type, bit_depth) {
        (ColorType::Rgba, png::BitDepth::Eight) => {
            // RGBA: 每像素 4 字节
            let pixel_count = width * height;
            let mut colors = Vec::with_capacity(pixel_count);
            for chunk in buf.chunks_exact(4) {
                colors.push([chunk[0], chunk[1], chunk[2], chunk[3]]);
            }
            colors
        }
        (ColorType::Rgb, png::BitDepth::Eight) => {
            // RGB: 每像素 3 字节，alpha 设为 255
            let pixel_count = width * height;
            let mut colors = Vec::with_capacity(pixel_count);
            for chunk in buf.chunks_exact(3) {
                colors.push([chunk[0], chunk[1], chunk[2], 255]);
            }
            colors
        }
        (ColorType::Indexed, _) => {
            // Indexed: 需要从 PLTE 和 tRNS 块重建颜色
            let pixel_count = width * height;
            let palette = palette_data.as_ref().ok_or_else(|| PngDecodeError {
                msg: "索引色 PNG 缺少 PLTE 块".to_string(),
            })?;

            let mut colors = Vec::with_capacity(pixel_count);
            for &idx in buf.iter().take(pixel_count) {
                let idx = idx as usize;
                if idx * 3 + 2 < palette.len() {
                    let r = palette[idx * 3];
                    let g = palette[idx * 3 + 1];
                    let b = palette[idx * 3 + 2];
                    let a = trns_data
                        .as_ref()
                        .and_then(|t| t.get(idx))
                        .copied()
                        .unwrap_or(255);
                    colors.push([r, g, b, a]);
                } else {
                    colors.push([0, 0, 0, 255]);
                }
            }
            colors
        }
        _ => {
            return Err(PngDecodeError {
                msg: format!(
                    "不支持的 PNG 格式: color_type={:?}, bit_depth={:?}",
                    color_type, bit_depth
                ),
            });
        }
    };

    if colors.is_empty() {
        return Err(PngDecodeError {
            msg: "调色板中没有颜色".to_string(),
        });
    }

    tracing::debug!(
        "[Palette] 加载: {}x{}, {} 种颜色",
        width,
        height,
        colors.len()
    );

    Ok(colors)
}

// ─── 全局当前调色板名称（由 UI 层在设置变更时更新） ───────────────────────

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

/// 当前活跃的调色板名称（缓存索引，减少字符串比较）
///
/// 每次设置变更时由 `set_current_palette_by_name` 更新。
static CURRENT_PALETTE_IDX: AtomicU8 = AtomicU8::new(0);
/// 当前调色板是否已初始化
static CURRENT_PALETTE_INIT: AtomicBool = AtomicBool::new(false);
/// MIDI 加载后调色板是否被锁定（禁止用户修改）
static PALETTE_LOCKED: AtomicBool = AtomicBool::new(false);

/// 获取当前调色板的轨道颜色（RGBA [u8;4]）
///
/// 如果尚未初始化，返回调色板管理器的默认颜色。
#[inline]
pub fn current_track_color(track_idx: usize) -> PaletteColor {
    if !CURRENT_PALETTE_INIT.load(Ordering::Relaxed) {
        return PALETTE_MANAGER.default().track_color(track_idx);
    }
    let idx = CURRENT_PALETTE_IDX.load(Ordering::Relaxed) as usize;
    PALETTE_MANAGER
        .palettes()
        .get(idx)
        .map(|p| p.track_color(track_idx))
        .unwrap_or_else(|| FALLBACK_PALETTE[track_idx % FALLBACK_PALETTE.len()])
}

/// 获取当前调色板的轨道颜色（[f32;4]，归一化到 0.0-1.0）
#[inline]
pub fn current_track_color_f32(track_idx: usize) -> [f32; 4] {
    let c = current_track_color(track_idx);
    [
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
        c[3] as f32 / 255.0,
    ]
}

/// 通过调色板名称设置当前调色板
///
/// 返回是否成功找到该调色板。
/// 如果调色板已被锁定（MIDI 加载后），返回 false 且不修改当前选择。
pub fn set_current_palette_by_name(name: &str) -> bool {
    if PALETTE_LOCKED.load(Ordering::Relaxed) {
        tracing::debug!("[Palette] 调色板已锁定（MIDI 加载后），忽略切换请求");
        return false;
    }
    let mgr = &*PALETTE_MANAGER;
    if let Some(idx) = mgr.palettes().iter().position(|p| p.name == name) {
        CURRENT_PALETTE_IDX.store(idx as u8, Ordering::Relaxed);
        CURRENT_PALETTE_INIT.store(true, Ordering::Relaxed);
        true
    } else {
        // 名称不存在，使用默认
        CURRENT_PALETTE_IDX.store(0, Ordering::Relaxed);
        CURRENT_PALETTE_INIT.store(true, Ordering::Relaxed);
        false
    }
}

/// 锁定当前调色板（MIDI 加载后调用，禁止用户修改）
pub fn lock_palette() {
    PALETTE_LOCKED.store(true, Ordering::Relaxed);
    tracing::info!("[Palette] 调色板已锁定");
}

/// 解锁调色板（关闭 MIDI 或应用重启时）
pub fn unlock_palette() {
    PALETTE_LOCKED.store(false, Ordering::Relaxed);
    tracing::info!("[Palette] 调色板已解锁");
}

/// 检查调色板是否被锁定
pub fn is_palette_locked() -> bool {
    PALETTE_LOCKED.load(Ordering::Relaxed)
}

/// 获取洋葱皮音轨颜色（RGBA [u8;4]）
///
/// 从当前调色板的第二个颜色开始取色（index 0 保留给主音轨音符），
/// 超出调色板颜色数时循环取色。
#[inline]
pub fn onion_track_color(track_idx: usize) -> PaletteColor {
    let mgr = &*PALETTE_MANAGER;
    // 使用 CURRENT_PALETTE_INIT/PALETTE_INIT 判断是否初始化
    let p = if !CURRENT_PALETTE_INIT.load(Ordering::Relaxed) {
        mgr.default()
    } else {
        let idx = CURRENT_PALETTE_IDX.load(Ordering::Relaxed) as usize;
        mgr.palettes().get(idx).unwrap_or_else(|| mgr.default())
    };
    // 从第二个颜色开始取色（offset = 1）
    if p.colors.len() <= 1 {
        // 如果调色板只有 1 种或 0 种颜色，用备用色
        FALLBACK_PALETTE[track_idx % FALLBACK_PALETTE.len()]
    } else {
        p.colors[(1 + track_idx) % p.colors.len()]
    }
}

/// 获取洋葱皮音轨颜色（[f32;4]，归一化到 0.0-1.0）
#[inline]
pub fn onion_track_color_f32(track_idx: usize) -> [f32; 4] {
    let c = onion_track_color(track_idx);
    [
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
        c[3] as f32 / 255.0,
    ]
}

/// 获取当前调色板名称
pub fn current_palette_name() -> &'static str {
    if !CURRENT_PALETTE_INIT.load(Ordering::Relaxed) {
        return PALETTE_MANAGER.default().name;
    }
    let idx = CURRENT_PALETTE_IDX.load(Ordering::Relaxed) as usize;
    PALETTE_MANAGER
        .palettes()
        .get(idx)
        .map(|p| p.name)
        .unwrap_or_else(|| PALETTE_MANAGER.default().name)
}

/// 重置当前调色板到默认
pub fn reset_current_palette() {
    CURRENT_PALETTE_IDX.store(0, Ordering::Relaxed);
    CURRENT_PALETTE_INIT.store(true, Ordering::Relaxed);
}

#[cfg(test)]
mod tests;
