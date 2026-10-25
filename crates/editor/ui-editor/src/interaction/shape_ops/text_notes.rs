//! 文字形状再生成（DRAW-029）
//!
//! 文字形状持久化为 `ShapeOp::Text` + `ShapeGeometry::rect`；本模块按保存时的
//! 文本框、参数、采样模式与卷帘方向重新生成音符，供形状编辑/重生成使用。

use lumino_core::shape::{ShapeModel, ShapeOp, TextLayoutMode, TextOp};
use lumino_editor_state::ShapeNote;
use lumino_editor_state::text_tool::{TextToolMode, TextToolState};

use crate::interaction::text_tool::{
    GlyphGrid, rasterize_text, rasterize_text_params, sample_to_notes_grid,
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
                    .any(|op| matches!(op, ShapeOp::Text(_)))
                    .then(|| (entry.model.id, entry.model.clone()))
            })
            .last()
    }

    /// 把已有文字形状载入文字工具，进入重新编辑态
    pub(crate) fn begin_text_shape_edit(&mut self, id: u64, model: &ShapeModel) {
        let Some(op) = model.ops.iter().find_map(|op| match op {
            ShapeOp::Text(op) => Some(op),
            _ => None,
        }) else {
            return;
        };
        let Some(rect) = model.geometry.rect else {
            return;
        };
        // 载入时先按当前参数重算一次音符指纹，供 edit_shape 按值清理旧音符。
        let notes = self.shape_notes_of(model, model.track as usize);
        if notes.is_empty() {
            return;
        }
        self.editor_state.shape_store.assign_notes(id, notes);

        let tt = &mut self.editor_state.text_tool;
        let key_lo = rect.key_lo.round().clamp(0.0, 255.0) as u16;
        let key_hi = rect.key_hi.round().clamp(0.0, 255.0) as u16;
        tt.set_drag(rect.tick_lo, rect.tick_hi, key_lo, key_hi);
        tt.active = true;
        tt.editing = true;
        tt.text = op.text.clone();
        tt.params = op.params.clone();
        tt.layout_mode = op.layout;
        tt.mode = if op.merged {
            TextToolMode::KeyRangeMerged
        } else {
            TextToolMode::Normal
        };
        tt.editing_shape = Some(id);
    }

    /// 由文字形状模型重新生成音符（不写文档）
    pub(crate) fn text_shape_notes(
        &self,
        model: &ShapeModel,
        op: &TextOp,
        track: usize,
    ) -> Vec<ShapeNote> {
        let Some(rect) = model.geometry.rect else {
            return Vec::new();
        };
        let mut tt = TextToolState::new();
        tt.text = op.text.clone();
        tt.params = op.params.clone();
        tt.layout_mode = op.layout;
        let key_lo = rect.key_lo.round().clamp(0.0, 255.0) as u16;
        let key_hi = rect.key_hi.round().clamp(0.0, 255.0) as u16;
        tt.set_drag(rect.tick_lo, rect.tick_hi, key_lo, key_hi);

        let snap = op.snap.max(1.0);
        let grid = GlyphGrid::from_state(&tt, snap, op.vertical);
        let occupancy = match op.layout {
            TextLayoutMode::LegacyBox => {
                rasterize_text(&op.text, grid.cols, grid.rows, &op.params.font)
            }
            TextLayoutMode::Parametric => {
                rasterize_text_params(&op.text, grid.cols, grid.rows, &op.params)
            }
        };
        let Some(occupancy) = occupancy else {
            return Vec::new();
        };
        sample_to_notes_grid(&occupancy, &grid, snap, op.merged)
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
}
