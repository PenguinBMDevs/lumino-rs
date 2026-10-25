//! 文字形状再生成（DRAW-029）
//!
//! 文字形状持久化为 `ShapeOp::Text` + `ShapeGeometry::rect`；本模块按保存时的
//! 文本框、参数、采样模式与卷帘方向重新生成音符，供形状编辑/重生成使用。

use lumino_core::shape::{ShapeModel, TextLayoutMode, TextOp};
use lumino_editor_state::ShapeNote;
use lumino_editor_state::text_tool::TextToolState;

use crate::interaction::text_tool::{
    GlyphGrid, rasterize_text, rasterize_text_params, sample_to_notes_grid,
};

impl crate::Editor {
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
