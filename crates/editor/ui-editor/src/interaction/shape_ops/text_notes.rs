//! 文字形状再生成（DRAW-029 / UI-023）
//!
//! 文字形状持久化为 `ShapeOp::TextV2`（新：带冻结的 `k`）或 `ShapeOp::Text`（旧：4×4 兼容）
//! 加 `ShapeGeometry::rect`；本模块按保存时的文本框、参数、采样模式、卷帘方向
//! 与屏幕比例重新生成音符，供形状编辑 / 重生成使用。
//!
//! **重烘焙稳定性**：新路径只读 `op.k` + `op.snap`（+ 当前工程 PPQ），与当前缩放无关；
//! 旧路径固定走 [`RasterScale::legacy`]，逐字节保持历史结果。

use lumino_core::shape::{ShapeModel, ShapeOp, ShapeRect, TextLayoutMode, TextOp, TextOpV2};
use lumino_editor_state::ShapeNote;
use lumino_editor_state::text_tool::TextToolState;

use crate::interaction::text_tool::{
    GlyphGrid, RasterScale, rasterize_text_for_tool, sample_to_notes_grid,
};

impl crate::Editor {
    /// 当前轨命中某个文字形状（返回 id + 模型快照）
    pub(crate) fn text_shape_at(&self, tick: f32, key: u16) -> Option<(u64, ShapeModel)> {
        let track = self.editor_state.data.current_track;
        self.editor_state
            .shape_store
            .iter()
            .filter(|entry| entry.is_visible() && entry.model.track as usize == track)
            .filter_map(|entry| {
                let rect = entry.model.geometry.rect?;
                if tick < rect.tick_lo
                    || tick > rect.tick_hi
                    || f32::from(key) < rect.key_lo
                    || f32::from(key) > rect.key_hi
                {
                    return None;
                }
                entry
                    .model
                    .ops
                    .iter()
                    .any(|op| matches!(op, ShapeOp::Text(_) | ShapeOp::TextV2(_)))
                    .then(|| (entry.model.id, entry.model.clone()))
            })
            .last()
    }

    /// 由**旧**文字形状模型重新生成音符（`ShapeOp::Text`：4×4 兼容方块采样）
    pub(crate) fn text_shape_notes(
        &self,
        model: &ShapeModel,
        op: &TextOp,
        track: usize,
    ) -> Vec<ShapeNote> {
        let Some(rect) = model.geometry.rect else {
            return Vec::new();
        };
        let tt = restore_text_state(&op.text, &op.params, op.layout, rect);
        let snap = op.snap.max(1.0);
        let grid = GlyphGrid::from_state(&tt, snap, op.vertical);
        if grid.cols == 0 || grid.rows == 0 {
            return Vec::new();
        }
        // 旧形状没有 k：固定 4×4 方格 + 填满框拉伸，与落盘时的行为一致
        let scale = RasterScale::legacy(op.vertical);
        let Some(occupancy) = rasterize_text_for_tool(&tt, &op.text, &grid, &scale) else {
            return Vec::new();
        };
        to_shape_notes(
            sample_to_notes_grid(&occupancy, &grid, snap, op.merged),
            track,
        )
    }

    /// 由**新**文字形状模型重新生成音符（`ShapeOp::TextV2`：按冻结的 `op.k` 复现比例）
    ///
    /// 同一 `TextV2` 在两个不同缩放视图下重烘焙，结果必须逐字段一致——这正是
    /// 把 `k` 冻进形状的原因。
    pub(crate) fn text_shape_notes_v2(
        &self,
        model: &ShapeModel,
        op: &TextOpV2,
        track: usize,
    ) -> Vec<ShapeNote> {
        let Some(rect) = model.geometry.rect else {
            return Vec::new();
        };
        let tt = restore_text_state(&op.text, &op.params, op.layout, rect);
        let snap = op.snap.max(1.0);
        let grid = GlyphGrid::from_state(&tt, snap, op.vertical);
        if grid.cols == 0 || grid.rows == 0 {
            return Vec::new();
        }
        let scale = RasterScale::from_k(
            op.vertical,
            grid.tick_step,
            f32::from(self.editor_state.view.ppq),
            op.k,
            grid.cols,
            grid.rows,
        );
        let Some(occupancy) = rasterize_text_for_tool(&tt, &op.text, &grid, &scale) else {
            return Vec::new();
        };
        to_shape_notes(
            sample_to_notes_grid(&occupancy, &grid, snap, op.merged),
            track,
        )
    }
}

/// 由形状快照还原文字工具状态（文本框几何走 `set_drag`，确认时才吸附）
fn restore_text_state(
    text: &str,
    params: &lumino_core::TextParams,
    layout: TextLayoutMode,
    rect: ShapeRect,
) -> TextToolState {
    let mut tt = TextToolState::new();
    tt.text = text.to_string();
    tt.params = params.clone();
    tt.layout_mode = layout;
    let key_lo = rect.key_lo.round().clamp(0.0, 255.0) as u16;
    let key_hi = rect.key_hi.round().clamp(0.0, 255.0) as u16;
    tt.set_drag(rect.tick_lo, rect.tick_hi, key_lo, key_hi);
    tt
}

/// `(tick, key, length)` → 形状音符（力度 100 / 通道 0，与工具确认同口径）
fn to_shape_notes(notes: Vec<(f32, u16, f32)>, track: usize) -> Vec<ShapeNote> {
    notes
        .into_iter()
        .map(|(tick, key, length)| ShapeNote {
            track,
            tick,
            key,
            length,
            velocity: 100,
            channel: 0,
        })
        .collect()
}
