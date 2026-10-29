//! `ShapeOp::Drawing` 的多笔画再生成（DRAW-033A）
//!
//! Drawing 是源算子：每条 stroke 先按自己的 `StrokeKind` 走既有单路径生成，
//! 合并后**只跑一次** Notes 阶段算子，避免 Glue/Chop 等被按 stroke 重复执行。
//! 单条闭合 stroke 直接复用完整单形状管线（包括 Areas）。
use lumino_core::shape::{
    DrawingParams, OpStage, ShapeFill, ShapeGeometry, ShapeKind, ShapeModel, ShapeOp, StrokeKind,
};
use lumino_editor_state::ShapeNote;
use lumino_editor_state::shape_ops::{NoteOpContext, apply_note_ops};

use crate::Editor;

impl Editor {
    /// 多笔画 Drawing → 音符指纹
    pub(super) fn drawing_notes(
        &self,
        model: &ShapeModel,
        params: &DrawingParams,
        track: usize,
    ) -> Vec<ShapeNote> {
        // 单条闭合 stroke（内置三形/单闭合用户图形）复用完整单路径管线；
        // 这样 Areas 的 box 坐标仍与 drawing 的单位框一致。
        if params.strokes.len() == 1 && params.stroke_closed(0) {
            let mut temp = self.stroke_model(model, params, 0, true);
            temp.ops = model.ops.clone();
            temp.ops.retain(|op| !matches!(op, ShapeOp::Drawing(_)));
            return self.shape_notes_of(&temp, track);
        }

        let mut notes = Vec::new();
        for index in 0..params.strokes.len() {
            let temp = self.stroke_model(model, params, index, false);
            let mut part = self.shape_notes_of(&temp, track);
            notes.append(&mut part);
        }
        let ppq = f64::from(self.editor_state.view.ppq);
        let note_ctx = NoteOpContext {
            glue_box: params.bbox(),
            ppq,
        };
        apply_note_ops(&mut notes, &model.ops, &note_ctx);
        super::fill_notes::unlock_area_channels(&mut notes);
        notes
    }

    /// 把 Drawing 的第 `index` 条 stroke 变成临时单形状模型。
    ///
    /// `keep_areas=false` 时清空 Areas；多笔画 drawing 的区域语义留给 033C，
    /// 首版先保证每条 stroke 的轮廓/填充生成不串段。
    fn stroke_model(
        &self,
        model: &ShapeModel,
        params: &DrawingParams,
        index: usize,
        keep_areas: bool,
    ) -> ShapeModel {
        let stroke = &params.strokes[index];
        let closed = params.stroke_closed(index);
        let role = params.role(index);
        let mut temp = ShapeModel::new(0, model.track, stroke_kind(stroke.kind), model.k);
        temp.name = model.name.clone();
        temp.geometry = ShapeGeometry {
            anchors: stroke.anchors.clone(),
            rect: stroke.rect,
            closed,
        };
        temp.style = model.style.clone();
        temp.style.colour_slot = role.colour_slot;
        if role.filled && closed {
            temp.style.fill = ShapeFill::Fill;
        }
        if !keep_areas {
            temp.style.areas.clear();
        }
        // 多笔画分支只保留 Geometry 段，Notes 段在合并后统一跑一次。
        temp.ops = model
            .ops
            .iter()
            .filter(|op| op.stage() == OpStage::Geometry && !matches!(op, ShapeOp::Drawing(_)))
            .cloned()
            .collect();
        temp
    }
}

fn stroke_kind(kind: StrokeKind) -> ShapeKind {
    match kind {
        StrokeKind::Line => ShapeKind::Line,
        StrokeKind::Polyline => ShapeKind::Polyline,
        StrokeKind::Freehand => ShapeKind::Freehand,
        StrokeKind::Curve => ShapeKind::Curve,
        StrokeKind::Arc => ShapeKind::Arc,
    }
}
