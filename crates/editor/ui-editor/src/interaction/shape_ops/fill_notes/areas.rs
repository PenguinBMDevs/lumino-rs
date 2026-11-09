//! 区域着色（DRAW-032）：上下文构建、色号→通道映射与解锁。
//!
//! 从 `fill_notes.rs` 拆出，保持单文件 <400 行。

use lumino_core::shape::{
    AREA_BORDER, AREA_LOCK_BASE, AREA_UNASSIGNED, COLOUR_SLOT_MAX, ShapeModel, ShapeRect,
    most_colours,
};
use lumino_editor_state::ShapeNote;
use lumino_editor_state::shape_ops::AreaContext;

use crate::Editor;

impl Editor {
    /// 由已展平路径构建区域上下文；`areas` 为空或没有有效着色点时返回 `None`
    pub(crate) fn area_context(
        &self,
        paths: &[Vec<(f64, f64)>],
        model: &ShapeModel,
    ) -> Option<AreaContext> {
        let box_ = area_box_of(paths)?;
        AreaContext::build(
            paths,
            box_,
            &model.style.areas,
            model.style.union,
            self.editor_state.use10,
        )
    }
}

/// 形状自身 box（不做 Glue 的 key ±0.5 扩张；区域坐标以真实几何为准）
fn area_box_of(paths: &[Vec<(f64, f64)>]) -> Option<ShapeRect> {
    let mut points = paths.iter().flatten();
    let &(first_tick, first_key) = points.next()?;
    let mut tick_lo = first_tick;
    let mut tick_hi = first_tick;
    let mut key_lo = first_key;
    let mut key_hi = first_key;
    for &(tick, key) in points {
        tick_lo = tick_lo.min(tick);
        tick_hi = tick_hi.max(tick);
        key_lo = key_lo.min(key);
        key_hi = key_hi.max(key);
    }
    Some(ShapeRect::normalized(
        tick_lo as f32,
        key_lo as f32,
        tick_hi as f32,
        key_hi as f32,
    ))
}

/// 区域色 → 生成期 channel；`None` = 该段不产音符
pub(super) fn area_channel(colour: u8, use10: bool, stroke: u8) -> Option<u8> {
    match colour {
        AREA_UNASSIGNED => Some(0),
        0 => None,
        // 颜色交界：替换该格填充，走描边通道（DRAW-032 `borders`）
        AREA_BORDER => Some(AREA_LOCK_BASE + stroke.min(COLOUR_SLOT_MAX)),
        value if value <= most_colours(use10) => Some(AREA_LOCK_BASE + area_midi_channel(value)),
        _ => None,
    }
}

/// 区域色号 → MIDI 通道索引
///
/// 1..=15 原样映射；16（仅启用通道 10 时）映射到 GM 通道 10（内部索引 9）。
fn area_midi_channel(colour: u8) -> u8 {
    if colour >= 16 { 9 } else { colour }
}

/// 生成结束后把区域锁定 channel 还原成合法 MIDI 通道
pub(crate) fn unlock_area_channels(notes: &mut [ShapeNote]) {
    for note in notes {
        if note.channel >= AREA_LOCK_BASE {
            note.channel -= AREA_LOCK_BASE;
        }
    }
}
