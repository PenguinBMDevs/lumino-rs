//! 020 Custom edit 的 UI 拖拽状态（CP-4.5a）
//!
//! 纯变换在 `editor_state::shape_edit`；本模块负责：
//! 命中手柄 → 拖动预览 → 松手提交。
//!
//! 041C 阶段 3：目标既可以是已 Bake 的 DrawnShape（走 `edit_shape`），
//! 也可以是单支 `PendingComposed`（只改 pending model + 预览音符）。

use iced_core::Point;
use lumino_core::shape::ShapeModel;
use lumino_editor_state::editor_state::{
    HandleKind, Mods, Zoom, apply_drag, edit_handles, hit_test,
};
use lumino_editor_state::{PendingComposed, PendingShapeRef};

use crate::Editor;

use super::shape_bridge::source_from_model;

/// 手柄编辑目标
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShapeEditTarget {
    /// 已 Bake 的 DrawnShape + ShapeStore 模型
    Drawn {
        /// UI 图形 id
        drawn_id: u64,
        /// ShapeStore 模型 id
        model_id: u64,
    },
    /// 单支 pending composed 结果
    PendingComposed {
        /// `EditorState::pending_composed` 下标
        index: usize,
    },
}

/// 一次形状手柄拖拽
#[derive(Debug, Clone)]
pub(crate) struct ShapeEditState {
    /// 编辑目标
    pub target: ShapeEditTarget,
    /// 当前手柄
    pub handle: HandleKind,
    /// 按下时的逻辑坐标
    pub start: (f32, f32),
    /// 拖动前模型（取消/判断空操作）
    pub original: ShapeModel,
    /// 拖动前的 pending 组合快照（仅 pending 目标；undo 用）
    pub original_composed: Vec<PendingComposed>,
    /// 当前预览模型
    pub current: ShapeModel,
    /// Ctrl / Shift
    pub mods: Mods,
}

impl Editor {
    /// 为当前唯一选中的图形绑定 ShapeModel，随后渲染编辑手柄。
    ///
    /// 041C 阶段 3：单支 pending composed 也允许进入手柄编辑。
    pub fn enable_shape_edit(&mut self) -> bool {
        if self.editor_state.shape_select.selection_len() != 1 {
            return false;
        }
        let Some(shape) = self.editor_state.shape_select.selected_shapes().next() else {
            return false;
        };
        let drawn_id = shape.id;
        let pending = shape.pending;
        let ok = match pending {
            Some(PendingShapeRef::Composed(index)) => self
                .editor_state
                .pending_composed
                .get(index)
                .is_some_and(|composed| composed.results.len() == 1),
            Some(_) => false,
            None => self.ensure_model_for_drawn(drawn_id).is_some(),
        };
        if ok {
            self.grid_cache.clear();
        }
        ok
    }

    /// 命中选中形状的手柄则进入编辑；返回是否接管本次按下
    pub(crate) fn begin_shape_edit(&mut self, tick: f32, key: f32) -> bool {
        if self.current_tool() != lumino_message::Tool::ShapeSelect
            || self.editor_state.shape_select.selection_len() != 1
        {
            return false;
        }
        let (drawn_id, pending, model_id) = {
            let Some(drawn) = self.editor_state.shape_select.selected_shapes().next() else {
                return false;
            };
            (drawn.id, drawn.pending, drawn.model_id)
        };

        let (target, model, original_composed) = match pending {
            Some(PendingShapeRef::Composed(index)) => {
                let Some(composed) = self.editor_state.pending_composed.get(index) else {
                    return false;
                };
                if composed.results.len() != 1 {
                    return false;
                }
                (
                    ShapeEditTarget::PendingComposed { index },
                    composed.results[0].model.clone(),
                    self.editor_state.pending_composed.clone(),
                )
            }
            Some(_) => return false,
            None => {
                let Some(model_id) = model_id else {
                    return false;
                };
                let Some(model) = self.editor_state.shape_store.model(model_id).cloned() else {
                    return false;
                };
                (
                    ShapeEditTarget::Drawn { drawn_id, model_id },
                    model,
                    Vec::new(),
                )
            }
        };

        let zoom = Zoom {
            x: self.editor_state.view.zoom_x,
            y: self.editor_state.view.zoom_y,
        };
        let Some(handle) = hit_test(&model, (tick, key), zoom, 8.0) else {
            return false;
        };
        self.shape_edit = Some(ShapeEditState {
            target,
            handle,
            start: (tick, key),
            original: model.clone(),
            original_composed,
            current: model,
            mods: Mods {
                ctrl: self.ctrl_pressed(),
                shift: self.shift_pressed(),
            },
        });
        true
    }

    /// 拖动手柄；返回是否更新
    pub(crate) fn update_shape_edit(&mut self, tick: f32, key: f32) -> bool {
        let Some(mut state) = self.shape_edit.take() else {
            return false;
        };
        let k = state.original.k;
        // 关键：每帧都从拖动前的 original 重算一次。
        // 若在 state.current 上重复 apply_drag(start→current)，Move/Resize/Turn 会把
        // 同一个 delta 累乘/累加，表现为抽搐与飞出屏幕。
        let mut next = state.original.clone();
        if !apply_drag(
            &mut next,
            state.handle,
            state.start,
            (tick, key),
            state.mods,
            k,
        ) {
            self.shape_edit = Some(state);
            return false;
        }
        if !finite_geometry(&next) {
            self.shape_edit = Some(state);
            return false;
        }
        state.current = next;
        let notes = self.shape_notes_of(&state.current, state.current.track as usize);
        match state.target {
            ShapeEditTarget::Drawn { drawn_id, .. } => {
                let source = source_from_model(&state.current);
                self.editor_state.shape_select.set_source(drawn_id, source);
            }
            ShapeEditTarget::PendingComposed { index } => {
                if let Some(part) = self
                    .editor_state
                    .pending_composed
                    .get_mut(index)
                    .and_then(|composed| composed.results.first_mut())
                {
                    part.model = state.current.clone();
                    part.notes = notes;
                }
                self.refresh_pending_mirror_geometry();
            }
        }
        self.grid_cache.clear();
        self.shape_edit = Some(state);
        true
    }

    /// 松手提交；无实际变化或失败时恢复原几何
    pub(crate) fn finish_shape_edit(&mut self) -> bool {
        let Some(state) = self.shape_edit.take() else {
            return false;
        };
        let changed = state.current != state.original;
        match state.target {
            ShapeEditTarget::Drawn { drawn_id, model_id } => {
                if !changed {
                    let source = source_from_model(&state.original);
                    self.editor_state.shape_select.set_source(drawn_id, source);
                    return false;
                }
                let ok = self.edit_shape(model_id, state.current);
                if !ok {
                    let source = source_from_model(&state.original);
                    self.editor_state.shape_select.set_source(drawn_id, source);
                }
                self.grid_cache.clear();
                ok
            }
            ShapeEditTarget::PendingComposed { index } => {
                if !changed {
                    self.restore_pending_part(index, &state.original);
                    self.refresh_pending_mirror_geometry();
                    self.grid_cache.clear();
                    return false;
                }
                let notes = self.shape_notes_of(&state.current, state.current.track as usize);
                if let Some(part) = self
                    .editor_state
                    .pending_composed
                    .get_mut(index)
                    .and_then(|composed| composed.results.first_mut())
                {
                    part.model = state.current;
                    part.notes = notes;
                }
                self.pending_edit_undo.push(state.original_composed);
                while self.pending_edit_undo.len() > 64 {
                    self.pending_edit_undo.remove(0);
                }
                self.pending_edit_redo.clear();
                self.pending_composed_redo.clear();
                self.refresh_pending_mirror_geometry();
                self.grid_cache.clear();
                true
            }
        }
    }

    /// 取消拖拽并恢复原几何（预留给 Esc）
    #[allow(dead_code)]
    pub(crate) fn cancel_shape_edit(&mut self) {
        let Some(state) = self.shape_edit.take() else {
            return;
        };
        match state.target {
            ShapeEditTarget::Drawn { drawn_id, .. } => {
                let source = source_from_model(&state.original);
                self.editor_state.shape_select.set_source(drawn_id, source);
            }
            ShapeEditTarget::PendingComposed { index } => {
                self.restore_pending_part(index, &state.original);
                self.refresh_pending_mirror_geometry();
            }
        }
        self.grid_cache.clear();
    }

    fn restore_pending_part(&mut self, index: usize, model: &ShapeModel) {
        let notes = self.shape_notes_of(model, model.track as usize);
        if let Some(part) = self
            .editor_state
            .pending_composed
            .get_mut(index)
            .and_then(|composed| composed.results.first_mut())
        {
            part.model = model.clone();
            part.notes = notes;
        }
    }

    /// 当前编辑/选中形状的手柄屏幕位置（用于渲染）
    pub(crate) fn shape_edit_handle_points(&self) -> Vec<Point> {
        let Some(drawn) = self.editor_state.shape_select.selected_shapes().next() else {
            return Vec::new();
        };
        let model = match drawn.pending {
            Some(PendingShapeRef::Composed(index)) => {
                let Some(composed) = self.editor_state.pending_composed.get(index) else {
                    return Vec::new();
                };
                let Some(part) = composed.results.first() else {
                    return Vec::new();
                };
                if composed.results.len() != 1 {
                    return Vec::new();
                }
                &part.model
            }
            Some(_) => return Vec::new(),
            None => {
                let Some(model_id) = drawn.model_id else {
                    return Vec::new();
                };
                let Some(model) = self.editor_state.shape_store.model(model_id) else {
                    return Vec::new();
                };
                model
            }
        };
        let zoom = Zoom {
            x: self.editor_state.view.zoom_x,
            y: self.editor_state.view.zoom_y,
        };
        let Some(layout) = edit_handles(model, zoom) else {
            return Vec::new();
        };
        layout
            .handles
            .iter()
            .map(|(_, pos)| self.line_pos_screen_pos(*pos))
            .collect()
    }

    /// 是否正在拖拽形状手柄
    pub(crate) fn is_shape_editing(&self) -> bool {
        self.shape_edit.is_some()
    }
}

fn finite_geometry(model: &ShapeModel) -> bool {
    model
        .geometry
        .anchors
        .iter()
        .all(|anchor| anchor.pos.0.is_finite() && anchor.pos.1.is_finite())
        && model.geometry.rect.is_none_or(|rect| {
            rect.tick_lo.is_finite()
                && rect.tick_hi.is_finite()
                && rect.key_lo.is_finite()
                && rect.key_hi.is_finite()
        })
}

#[cfg(test)]
mod tests;
