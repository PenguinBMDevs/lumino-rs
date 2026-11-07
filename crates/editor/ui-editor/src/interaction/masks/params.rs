//! Mask 参数面板目标与写入接线（DRAW-010）
//!
//! 从 `masks.rs` 拆出（文件长度纪律）。

use lumino_core::shape::{MaskParams, ShapeOp};
use lumino_editor_state::PendingShapeRef;
use lumino_editor_state::shape_ops::masks as mask_ops;

use crate::Editor;

/// 遮罩参数面板的编辑目标
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskEditTarget {
    /// 待确认形状（`ShapeInstance.mask`）：预览即时生效，不写文档历史
    Pending {
        /// `shape_tool.shapes` 下标
        index: usize,
    },
    /// 单支待确认组合（020 提升出来的 `Single`）：同样即时预览
    PendingComposed {
        /// `EditorState::pending_composed` 下标
        index: usize,
    },
    /// 已 Bake 形状（`ShapeOp::Mask`）：面板改草稿，Apply 时一次 `edit_shape`
    Baked {
        /// ShapeStore 模型 id
        model_id: u64,
    },
}

impl Editor {
    /// 当前选中对象的遮罩编辑目标（须单选中；多支组合/曲线路径不支持）
    pub fn mask_edit_target(&mut self) -> Option<MaskEditTarget> {
        if let Some(shape) = self.editor_state.shape_select.selected_shapes().next() {
            if self.editor_state.shape_select.selection_len() != 1 {
                return None;
            }
            match shape.pending {
                Some(PendingShapeRef::Shape(index)) => {
                    return Some(MaskEditTarget::Pending { index });
                }
                Some(PendingShapeRef::Composed(index)) => {
                    let composed = self.editor_state.pending_composed.get(index)?;
                    if composed.kind != lumino_editor_state::ComposedKind::Single {
                        return None;
                    }
                    return Some(MaskEditTarget::PendingComposed { index });
                }
                Some(_) => return None,
                None => {}
            }
        }
        let (_, model_id, _) = self.selected_model_for_edit()?;
        Some(MaskEditTarget::Baked { model_id })
    }

    /// 目标当前的遮罩参数（无遮罩 → `None`）
    pub fn mask_params_of(&self, target: MaskEditTarget) -> Option<MaskParams> {
        match target {
            MaskEditTarget::Pending { index } => self
                .editor_state
                .shape_tool
                .shapes
                .get(index)
                .and_then(|shape| shape.mask.clone()),
            MaskEditTarget::PendingComposed { index } => self
                .editor_state
                .pending_composed
                .get(index)
                .and_then(|composed| composed.results.first())
                .and_then(|part| mask_ops::params_of(&part.model).cloned()),
            MaskEditTarget::Baked { model_id } => self
                .editor_state
                .shape_store
                .model(model_id)
                .and_then(|model| mask_ops::params_of(model).cloned()),
        }
    }

    /// 待确认形状即时设置遮罩（`None` = 清除）；已 Bake 请走 [`Self::set_mask`]
    pub fn set_pending_mask(&mut self, index: usize, params: Option<MaskParams>) -> bool {
        let Some(shape) = self.editor_state.shape_tool.shapes.get_mut(index) else {
            return false;
        };
        if shape.mask == params {
            return false;
        }
        shape.mask = params;
        self.rebuild_pending_mirrors();
        self.mark_notes_changed();
        self.grid_cache.clear();
        true
    }

    /// 单支待确认组合的遮罩：改写模型上的 `Mask` op 并重算预览音符
    pub fn set_pending_composed_mask(&mut self, index: usize, params: Option<MaskParams>) -> bool {
        let Some(composed) = self.editor_state.pending_composed.get(index) else {
            return false;
        };
        let Some(mut model) = composed.results.first().map(|part| part.model.clone()) else {
            return false;
        };
        model.ops.retain(|op| !matches!(op, ShapeOp::Mask(_)));
        if let Some(params) = params {
            model.ops.push(ShapeOp::Mask(params));
        }
        let track = model.track as usize;
        let notes = self.shape_notes_of(&model, track);
        let Some(part) = self
            .editor_state
            .pending_composed
            .get_mut(index)
            .and_then(|composed| composed.results.first_mut())
        else {
            return false;
        };
        part.model = model;
        part.notes = notes;
        self.refresh_pending_mirror_geometry();
        self.mark_notes_changed();
        self.grid_cache.clear();
        true
    }

    /// 给形状设置/移除 Mask 标记（供工具栏/面板接线；一次 edit_shape 撤销单元）
    pub fn set_mask(&mut self, id: u64, params: Option<MaskParams>) -> bool {
        let Some(mut model) = self.editor_state.shape_store.model(id).cloned() else {
            return false;
        };
        model.ops.retain(|op| !matches!(op, ShapeOp::Mask(_)));
        if let Some(params) = params {
            model.ops.push(ShapeOp::Mask(params));
        }
        self.edit_shape(id, model)
    }

    /// 切换当前选中形状的 Mask 标记：pending 形状改 `ShapeInstance.mask`；
    /// 已 Bake 形状走 `set_mask`（一次 edit_shape 撤销单元）。
    pub fn toggle_mask_selected(&mut self) -> bool {
        if self.pending_single_selection() {
            return self.toggle_pending_mask();
        }
        let Some((_, model_id, model)) = self.selected_model_for_edit() else {
            return false;
        };
        let params = match mask_ops::params_of(&model) {
            Some(_) => None,
            None => Some(MaskParams::default()),
        };
        self.set_mask(model_id, params)
    }

    fn toggle_pending_mask(&mut self) -> bool {
        let index = {
            let mut selected = self.editor_state.shape_select.selected_shapes();
            let Some(shape) = selected.next() else {
                return false;
            };
            if selected.next().is_some() {
                return false;
            }
            let Some(PendingShapeRef::Shape(index)) = shape.pending else {
                return false;
            };
            index
        };
        {
            let Some(shape) = self.editor_state.shape_tool.shapes.get_mut(index) else {
                return false;
            };
            shape.mask = if shape.mask.is_some() {
                None
            } else {
                Some(MaskParams::default())
            };
        }
        self.rebuild_pending_mirrors();
        self.grid_cache.clear();
        self.mark_notes_changed();
        true
    }
}
