//! 文字形状再生成（DRAW-029 / UI-023）
//!
//! 文字形状持久化为 `ShapeOp::Text`（旧，无 k）或 `ShapeOp::TextV2`（新，冻结
//! 绘制时的 `k`）+ `ShapeGeometry::rect`；本模块按保存时的文本框、参数、采样
//! 模式、卷帘方向与 `k` 重新生成音符。
//!
//! **旧 `Text` 必须逐字节保持旧行为**：用 `RasterScale::legacy` 的 4×4 方格 +
//! 旧填满 / 面积阈值路径；新 `TextV2` 用 `from_k` 的非正方形采样 + key 轴阈值。

use lumino_core::shape::{ShapeModel, ShapeOp, TextLayoutMode, TextOp, TextOpV2, TextParams};
use lumino_editor_state::ShapeNote;
use lumino_editor_state::text_tool::TextToolState;

use crate::interaction::text_tool::{
    GlyphGrid, RasterScale, rasterize_text, rasterize_text_params, sample_to_notes_grid,
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

    /// 旧 `Text`（v1）：legacy 4×4 方格，保持修复前逐字节行为
    pub(crate) fn text_shape_notes(
        &self,
        model: &ShapeModel,
        op: &TextOp,
        track: usize,
    ) -> Vec<ShapeNote> {
        self.text_shape_notes_with(
            model,
            &op.text,
            &op.params,
            op.layout,
            op.snap,
            op.merged,
            op.vertical,
            None,
            track,
        )
    }

    /// 新 `TextV2`：用冻结的 `op.k` 推非正方形采样，跨 zoom 重烘焙稳定
    pub(crate) fn text_shape_notes_v2(
        &self,
        model: &ShapeModel,
        op: &TextOpV2,
        track: usize,
    ) -> Vec<ShapeNote> {
        self.text_shape_notes_with(
            model,
            &op.text,
            &op.params,
            op.layout,
            op.snap,
            op.merged,
            op.vertical,
            Some(op.k),
            track,
        )
    }

    /// 文字再生成公共路径：`frozen_k = None` 表示旧 v1 legacy 方格。
    #[allow(clippy::too_many_arguments)]
    fn text_shape_notes_with(
        &self,
        model: &ShapeModel,
        text: &str,
        params: &TextParams,
        layout: TextLayoutMode,
        snap: f32,
        merged: bool,
        vertical: bool,
        frozen_k: Option<f32>,
        track: usize,
    ) -> Vec<ShapeNote> {
        let Some(rect) = model.geometry.rect else {
            return Vec::new();
        };
        let mut tt = TextToolState::new();
        tt.text = text.to_string();
        tt.params = params.clone();
        tt.layout_mode = layout;
        let key_lo = rect.key_lo.round().clamp(0.0, 255.0) as u16;
        let key_hi = rect.key_hi.round().clamp(0.0, 255.0) as u16;
        tt.set_drag(rect.tick_lo, rect.tick_hi, key_lo, key_hi);

        let snap = snap.max(1.0);
        let grid = GlyphGrid::from_state(&tt, snap, vertical);
        if grid.cols == 0 || grid.rows == 0 {
            return Vec::new();
        }
        let scale = match frozen_k {
            Some(k) => {
                let ppq = f32::from(self.editor_state.view.ppq).max(1.0);
                RasterScale::from_k(vertical, snap, ppq, k, grid.cols, grid.rows)
            }
            None => RasterScale::legacy(vertical),
        };
        let occupancy = match layout {
            TextLayoutMode::LegacyBox => rasterize_text(text, grid.cols, grid.rows, params, scale),
            TextLayoutMode::Parametric => {
                rasterize_text_params(text, grid.cols, grid.rows, params, scale)
            }
        };
        let Some(occupancy) = occupancy else {
            return Vec::new();
        };
        sample_to_notes_grid(&occupancy, &grid, snap, merged)
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
