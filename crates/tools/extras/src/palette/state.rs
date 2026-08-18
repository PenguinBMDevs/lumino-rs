//! 全局当前调色板状态
//!
//! 使用原子变量缓存当前调色板索引与锁定状态，避免每次取色做字符串比较。

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use super::{FALLBACK_PALETTE, PALETTE_MANAGER, PaletteColor};

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
        .map(|palette| palette.track_color(track_idx))
        .unwrap_or_else(|| FALLBACK_PALETTE[track_idx % FALLBACK_PALETTE.len()])
}

/// 获取当前调色板的轨道颜色（[f32;4]，归一化到 0.0-1.0）
#[inline]
pub fn current_track_color_f32(track_idx: usize) -> [f32; 4] {
    let color = current_track_color(track_idx);
    [
        color[0] as f32 / 255.0,
        color[1] as f32 / 255.0,
        color[2] as f32 / 255.0,
        color[3] as f32 / 255.0,
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
    if let Some(idx) = mgr
        .palettes()
        .iter()
        .position(|palette| palette.name == name)
    {
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

/// 获取当前调色板索引（用于变更检测）
///
/// 返回 `u8` 索引值。如果调色板未初始化，返回 0。
/// 调用方可以缓存此值并在下次比较，不同则表示调色板已切换，需要重建依赖项
///（如洋葱皮实例）。
#[inline]
pub fn current_palette_idx() -> u8 {
    CURRENT_PALETTE_IDX.load(Ordering::Relaxed)
}

/// 获取洋葱皮音轨颜色（RGBA [u8;4]）
///
/// 从当前调色板的第一个颜色开始取色，
/// 超出调色板颜色数时循环取色。
#[inline]
pub fn onion_track_color(track_idx: usize) -> PaletteColor {
    let mgr = &*PALETTE_MANAGER;
    // 使用 CURRENT_PALETTE_INIT/PALETTE_INIT 判断是否初始化
    let palette = if !CURRENT_PALETTE_INIT.load(Ordering::Relaxed) {
        mgr.default()
    } else {
        let idx = CURRENT_PALETTE_IDX.load(Ordering::Relaxed) as usize;
        mgr.palettes().get(idx).unwrap_or_else(|| mgr.default())
    };
    // 从第一个颜色开始取色（offset = 0）
    if palette.colors.is_empty() {
        // 如果调色板没有颜色，用备用色
        FALLBACK_PALETTE[track_idx % FALLBACK_PALETTE.len()]
    } else {
        palette.colors[track_idx % palette.colors.len()]
    }
}

/// 获取洋葱皮音轨颜色（[f32;4]，归一化到 0.0-1.0）
#[inline]
pub fn onion_track_color_f32(track_idx: usize) -> [f32; 4] {
    let color = onion_track_color(track_idx);
    [
        color[0] as f32 / 255.0,
        color[1] as f32 / 255.0,
        color[2] as f32 / 255.0,
        color[3] as f32 / 255.0,
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
        .map(|palette| palette.name)
        .unwrap_or_else(|| PALETTE_MANAGER.default().name)
}

/// 重置当前调色板到默认
pub fn reset_current_palette() {
    CURRENT_PALETTE_IDX.store(0, Ordering::Relaxed);
    CURRENT_PALETTE_INIT.store(true, Ordering::Relaxed);
}
