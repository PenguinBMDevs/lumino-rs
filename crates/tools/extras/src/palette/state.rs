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

/// 获取当前调色板的轨道颜色（RGBA \[u8;4\]）
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

/// 获取当前调色板的轨道颜色（\[f32;4\]，归一化到 0.0-1.0）
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

/// 主音轨（当前编辑轨）音符的固定蓝色
///
/// **必须与 4 个 shader 的 `MAIN_TRACK_COLOR` 保持一致**：
/// `note_renderer` / `onion_note` / `onion_note_vertical` / `splat` 均硬编码
/// `vec3<f32>(0.2, 0.55, 1.0)`。改这里的 u8 值而不改 shader 会造成
/// 「键盘键色」与「卷帘音符色」再次分叉。
pub const MAIN_TRACK_NOTE_COLOR: PaletteColor = [51, 140, 255, 255];

/// 主音轨音符固定蓝色（`[f32; 4]`，与 shader 常量逐位一致）
pub const MAIN_TRACK_NOTE_COLOR_F32: [f32; 4] = [0.2, 0.55, 1.0, 1.0];

/// 卷帘音符颜色（RGB-A，`u8`）：**全工程音符取色的唯一权威**。
///
/// 卷帘的着色规则是「两段式」的，键盘高亮与视频导出键盘必须照此规则取色，
/// 否则会出现「同一轨的键色 ≠ 音符色」：
///
/// - `doc_track == current_track`（主音轨 / 当前编辑轨）→ [`MAIN_TRACK_NOTE_COLOR`]；
/// - 其他轨 → [`current_track_color`]（调色板按音轨索引循环取色）。
///
/// 传入 `current_track = usize::MAX` 表示「无主音轨概念」（视频导出等整篇渲染
/// 路径），此时所有轨一律取调色板色。
#[inline]
pub fn note_track_color(doc_track: usize, current_track: usize) -> PaletteColor {
    if doc_track == current_track {
        MAIN_TRACK_NOTE_COLOR
    } else {
        current_track_color(doc_track)
    }
}

/// 卷帘音符颜色（`[f32; 4]`，归一化到 0.0-1.0）
#[inline]
pub fn note_track_color_f32(doc_track: usize, current_track: usize) -> [f32; 4] {
    if doc_track == current_track {
        return MAIN_TRACK_NOTE_COLOR_F32;
    }
    current_track_color_f32(doc_track)
}

/// 获取洋葱皮音轨颜色（RGBA \[u8;4\]）
///
/// 从当前调色板的第一个颜色开始取色，
/// 超出调色板颜色数时循环取色。
///
/// **与 [`current_track_color`] 等价**：两者都是 `colors[track_idx % len]`，
/// 此处保留独立函数名只为表达「洋葱皮语义」，实现委托以免调色板取色逻辑
/// 出现第二份拷贝（历史上两份实现曾在 `palettes().get(idx)` 落空时给出
/// 不同备用色）。
#[inline]
pub fn onion_track_color(track_idx: usize) -> PaletteColor {
    current_track_color(track_idx)
}

/// 获取洋葱皮音轨颜色（\[f32;4\]，归一化到 0.0-1.0）
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
