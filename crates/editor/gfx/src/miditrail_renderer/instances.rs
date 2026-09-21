//! Miditrail 3D 渲染器实例构建

use super::MIDITRAIL_SCENE_DEPTH;
use super::types::{
    MiditrailAuraInstanceGpu, MiditrailDrivenParamsGpu, MiditrailInstanceGpu, MiditrailNoteGpu,
    MiditrailUniformGpu,
};
use crate::NoteInstance;
use crate::is_black_key;

mod aura;
mod driven;
mod keys;
mod notes;

pub use aura::{build_aura_instances, compute_active_and_aura_for_compact, emit_aura_instances};
pub use driven::build_driven_params;
pub use keys::{build_key_instances, compute_active_keys, update_key_positions};
pub use notes::build_note_instances;

const KEYBOARD_HEIGHT: f32 = 0.012;
const WHITE_KEY_DEPTH: f32 = 0.07;
const BLACK_KEY_DEPTH: f32 = 0.0448;
const NOTE_HEIGHT: f32 = 0.007;
const NOTE_Y: f32 = 0.0005;
const NOTE_Z_OFFSET: f32 = 0.012;
const BLACK_KEY_ELEVATION: f32 = 0.0;
const BLACK_KEY_HEIGHT: f32 = 0.024;
const BLACK_KEY_WIDTH_RATIO: f32 = 0.58;

// ── Aura 光晕环动画参数（参考 Zenith-MIDI MidiTrailRender/Render.cs）──

/// 光环半径 = 键宽 × 该系数 × 光晕系数。
///
/// Zenith 原式为 `circleRadius * 12 * auraSize`；按视觉反馈缩到其 2/3（12 → 8）：
/// 常态尺寸回到 4 倍键宽（与动画化之前一致），按下闪光峰值 = 8 × 2/3 ≈ 5.33 倍键宽。
const AURA_RING_SCALE: f32 = 8.0;
/// 按下闪光在起始后多少帧内二次衰减到 0（Zenith 硬编码 10）。
const AURA_FLASH_FRAMES: f32 = 10.0;
/// 闪光分量缩放系数：起始峰值 = 100 / 600 ≈ 0.167。
const AURA_FLASH_DIVISOR: f32 = 600.0;
/// 常态/收缩分量的时间基准（秒）：剩余时长超过该值时光环保持常态尺寸。
const AURA_TAIL_SECONDS: f32 = 1.0;
/// 收缩分量幂指数：`(剩余时长 / 音符时长) ^ 0.3`。
const AURA_TAIL_POWER: f32 = 0.3;
/// 长音符保持期（剩余时长 ≥ 1s）的光环系数。
const AURA_HELD_FACTOR: f32 = 0.5;

/// 当前 tick 下被按下的键信息（同一键多个音符时取最后一个音符颜色）。
#[derive(Debug, Clone, Copy)]
pub struct ActiveKeys {
    /// 每个键是否被按下。
    pub pressed: [bool; 128],
    /// 每个键的激活颜色。
    pub colors: [u32; 128],
}

/// 将 [r, g, b, a] 颜色打包为 `0xRRGGBBAA`。
#[must_use]
pub fn pack_color(color: [f32; 4]) -> u32 {
    let r = (color[0].clamp(0.0, 1.0) * 255.0) as u32;
    let g = (color[1].clamp(0.0, 1.0) * 255.0) as u32;
    let b = (color[2].clamp(0.0, 1.0) * 255.0) as u32;
    let a = (color[3].clamp(0.0, 1.0) * 255.0) as u32;
    (r << 24) | (g << 16) | (b << 8) | a
}

/// 将颜色每个通道提亮指定值并重新打包为 `0xRRGGBBAA`。
///
/// 参考 Comet MIDITrail：激活音符在最终颜色上直接 +0.5（clamp 到 1.0）。
#[must_use]
pub fn boost_color_packed(packed: u32, amount: f32) -> u32 {
    let a = packed & 0xFF;
    let r = (((packed >> 24) & 0xFF) as f32 / 255.0 + amount).clamp(0.0, 1.0);
    let g = (((packed >> 16) & 0xFF) as f32 / 255.0 + amount).clamp(0.0, 1.0);
    let b = (((packed >> 8) & 0xFF) as f32 / 255.0 + amount).clamp(0.0, 1.0);
    (((r * 255.0) as u32) << 24) | (((g * 255.0) as u32) << 16) | (((b * 255.0) as u32) << 8) | a
}

/// `build_note_instances` 跨帧复用的暂存集（调用方持有，零每帧分配）。
///
/// 四块缓冲各司其职：`order` 存 (排序键, 下标) 供 radix 排序；`gather`
/// 存按序 gather 后的实例（与 `out` swap）；`radix` 是基数排序 ping-pong
/// 副缓冲；`hist` 复用 65536 桶直方图。由渲染器/测试持有并跨帧复用。
#[derive(Debug, Default)]
pub struct NoteBuildScratch {
    /// (排序键, `out` 下标)，radix 排序输入/输出。
    pub order: Vec<(u64, u32)>,
    /// gather 暂存（与 `out` swap）。
    pub gather: Vec<MiditrailInstanceGpu>,
    /// radix ping-pong 副缓冲。
    pub radix: Vec<(u64, u32)>,
    /// 65536 桶直方图复用。
    pub hist: Vec<u32>,
}

/// LSD 基数排序（16bit × 4 pass，稳定）：对 (排序键, 下标) 按键排序。
///
/// 与 `sort_by_key`（稳定）输出严格一致：LSD 从低位到高位逐 pass 稳定
/// 分桶，同键保持输入相对顺序；排序键是全序 u64，不存在"相等但比较器
/// 不一致"的暗坑。4 pass 为偶数，ping-pong 后结果落回 `order`，无需回拷。
/// 每 pass 流量 ≈ 键读 8B + 下标读写 8B，36 万音符约 23MB，远小于比较排序
/// O(n log n) 次 16B 元组搬运。直方图 65536 × u32 由调用方复用。
fn radix_sort_order(order: &mut Vec<(u64, u32)>, tmp: &mut Vec<(u64, u32)>, hist: &mut Vec<u32>) {
    const BITS: u32 = 16;
    const BUCKETS: usize = 1 << BITS;
    const PASSES: u32 = 64 / BITS;
    let n = order.len();
    if n < 2 {
        return;
    }
    tmp.clear();
    tmp.resize(n, (0, 0));
    hist.clear();
    hist.resize(BUCKETS, 0);
    let (mut src, mut dst) = (order.as_mut_slice(), tmp.as_mut_slice());
    for pass in 0..PASSES {
        let shift = pass * BITS;
        hist.fill(0);
        for &(key, _) in src.iter() {
            hist[((key >> shift) & 0xFFFF) as usize] += 1;
        }
        let mut sum = 0u32;
        for count in hist.iter_mut() {
            let c = *count;
            *count = sum;
            sum += c;
        }
        for &(key, idx) in src.iter() {
            let b = ((key >> shift) & 0xFFFF) as usize;
            let pos = hist[b] as usize;
            hist[b] = pos as u32 + 1;
            dst[pos] = (key, idx);
        }
        std::mem::swap(&mut src, &mut dst);
    }
    // PASSES = 4 为偶数：偶数次 swap 后 `src` 指回 `order` 的缓冲，结果已就位。
}

#[cfg(test)]
mod tests;
