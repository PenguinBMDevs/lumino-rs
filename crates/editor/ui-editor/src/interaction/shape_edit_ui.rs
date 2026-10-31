//! 020 Custom edit 的 UI 拖拽状态（CP-4.5a）
//!
//! 纯变换在 `editor_state::shape_edit`；本模块负责：
//! 命中手柄 → 拖动预览 → 松手提交。
//!
//! 041C 阶段 3：目标既可以是已 Bake 的 DrawnShape（走 `edit_shape`），
//! 也可以是单支 `PendingComposed`（只改 pending model + 预览音符）。

use iced_core::Point;
use lumino_core::shape::{ShapeModel, ShapeOp};
use lumino_editor_state::editor_state::{
    HandleKind, Mods, Zoom, apply_drag, edit_handles, hit_test,
};
use lumino_editor_state::{ComposedKind, PendingComposed, PendingShapeRef, ShapeNote};

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
    /// 041C 阶段 3：只有能可靠重生成的单支组合才开放手柄。
    ///
    /// Merge 的跨形状位移不在模型里，单独改第一支会丢第二支音符；Slice/Between
    /// 是多支，暂不提供整体手柄。
    fn pending_handle_supported(composed: &PendingComposed) -> bool {
        composed.results.len() == 1
            && matches!(
                composed.kind,
                ComposedKind::Live
                    | ComposedKind::Join
                    | ComposedKind::CopyAlong
                    | ComposedKind::Velocity
            )
    }

    /// 为当前唯一选中的图形进入/退出手柄编辑模式。
    ///
    /// 041C 阶段 3：单支 pending composed 也允许进入。显式模式避免「中心 Move
    /// 手柄」抢占普通框选/整组拖动。
    pub fn enable_shape_edit(&mut self) -> bool {
        if self.editor_state.shape_select.selection_len() != 1 {
            return false;
        }
        let (drawn_id, pending, model_id) = {
            let Some(shape) = self.editor_state.shape_select.selected_shapes().next() else {
                return false;
            };
            (shape.id, shape.pending, shape.model_id)
        };
        let target = match pending {
            Some(PendingShapeRef::Composed(index)) => {
                let Some(composed) = self.editor_state.pending_composed.get(index) else {
                    return false;
                };
                if !Self::pending_handle_supported(composed) {
                    return false;
                }
                ShapeEditTarget::PendingComposed { index }
            }
            Some(_) => return false,
            None => {
                let model_id = match model_id {
                    Some(id) => id,
                    None => match self.ensure_model_for_drawn(drawn_id) {
                        Some(id) => id,
                        None => return false,
                    },
                };
                ShapeEditTarget::Drawn { drawn_id, model_id }
            }
        };
        self.shape_edit_armed = if self.shape_edit_armed == Some(target) {
            None
        } else {
            Some(target)
        };
        self.grid_cache.clear();
        true
    }

    /// 命中手柄则进入拖拽；必须先通过「编辑形状」进入显式编辑模式。
    pub(crate) fn begin_shape_edit(&mut self, tick: f32, key: f32) -> bool {
        if self.current_tool() != lumino_message::Tool::ShapeSelect
            || self.editor_state.shape_select.selection_len() != 1
        {
            return false;
        }
        let Some(armed) = self.shape_edit_armed else {
            return false;
        };
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
                if !Self::pending_handle_supported(composed) {
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
        if target != armed {
            return false;
        }

        let zoom = Zoom {
            x: self.editor_state.view.zoom_x,
            y: self.editor_state.view.zoom_y,
        };
        let Some(handle) = hit_test(&model, (tick, key), zoom, 10.0) else {
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
        let k = match state.handle {
            // 旋转角必须在屏幕/拍–key 同一单位下计算；ShapeModel.k 是 beats per key。
            HandleKind::Turn => state.original.k.max(1e-6) * f32::from(self.editor_state.view.ppq),
            _ => state.original.k,
        };
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
        let notes = self.part_notes_for_edit(&state.current, state.current.track as usize);
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
                let notes = self.part_notes_for_edit(&state.current, state.current.track as usize);
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
        self.shape_edit_armed = None;
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
        let notes = self.part_notes_for_edit(model, model.track as usize);
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

    /// Live 的结果音符来自各 `from` 的原始音符；直接按展平几何重生成会得到
    /// 一条错误的连接线。其余单支模型可按当前 model 重生成。
    fn part_notes_for_edit(&self, model: &ShapeModel, track: usize) -> Vec<ShapeNote> {
        if let Some(ShapeOp::Live(params)) =
            model.ops.iter().find(|op| matches!(op, ShapeOp::Live(_)))
            && !params.from.is_empty()
        {
            let mut notes = Vec::new();
            for from in &params.from {
                notes.extend(self.shape_notes_of(from, from.track as usize));
            }
            notes
        } else {
            self.shape_notes_of(model, track)
        }
    }

    /// 当前编辑/选中形状的手柄屏幕位置（用于渲染）
    pub(crate) fn shape_edit_handle_points(&self) -> Vec<Point> {
        let Some(armed) = self.shape_edit_armed else {
            return Vec::new();
        };
        let Some(drawn) = self.editor_state.shape_select.selected_shapes().next() else {
            return Vec::new();
        };
        let (target, model) = match drawn.pending {
            Some(PendingShapeRef::Composed(index)) => {
                let Some(composed) = self.editor_state.pending_composed.get(index) else {
                    return Vec::new();
                };
                if !Self::pending_handle_supported(composed) {
                    return Vec::new();
                }
                let Some(part) = composed.results.first() else {
                    return Vec::new();
                };
                (ShapeEditTarget::PendingComposed { index }, &part.model)
            }
            Some(_) => return Vec::new(),
            None => {
                let Some(model_id) = drawn.model_id else {
                    return Vec::new();
                };
                let Some(model) = self.editor_state.shape_store.model(model_id) else {
                    return Vec::new();
                };
                (
                    ShapeEditTarget::Drawn {
                        drawn_id: drawn.id,
                        model_id,
                    },
                    model,
                )
            }
        };
        if target != armed {
            return Vec::new();
        }
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

    /// 是否正在显式手柄编辑模式（用于菜单状态/Toast）
    pub fn is_shape_edit_armed(&self) -> bool {
        self.shape_edit_armed.is_some()
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
