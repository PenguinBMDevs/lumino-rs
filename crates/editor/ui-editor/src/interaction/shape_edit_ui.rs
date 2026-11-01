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

    /// 命中手柄/旋转区则进入拖拽；选择框内部仍交给普通整组移动。
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
                let model_id = match model_id {
                    Some(id) => id,
                    None => match self.ensure_model_for_drawn(drawn_id) {
                        Some(id) => id,
                        None => return false,
                    },
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
        let screen = self.line_pos_screen_pos((tick, key));
        let inside_box = self
            .shape_bounds_screen(&model)
            .is_some_and(|rect| Self::point_near_rect(rect, screen, 2.0));
        let handle = hit_test(&model, (tick, key), zoom, 10.0).filter(|handle| {
            self.handle_allowed(&model, *handle)
                && (inside_box || matches!(handle, HandleKind::AnchorHandle(_) | HandleKind::Path))
        });
        let handle = match handle {
            Some(handle) => handle,
            None => {
                // 角外旋转：对角线外侧一圈，类似画板/Office 的选框外旋转区。
                if !self.box_handles_enabled(&model)
                    || !self.in_shape_rotate_zone(&model, (tick, key))
                {
                    return false;
                }
                HandleKind::Turn
            }
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
        if !finite_geometry(&next) || !sane_geometry(&next) {
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
        let Some(drawn) = self.editor_state.shape_select.selected_shapes().next() else {
            return Vec::new();
        };
        let model = match drawn.pending {
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
            .filter(|(kind, _)| self.handle_allowed(model, *kind))
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

/// 几何数值范围守卫：防止旋转/缩放的维度错误把 key 写成几百万，
/// 后续填充生成按 key 行循环直接吃满内存（曾经导致 MEMORY_ABORT）。
fn sane_geometry(model: &ShapeModel) -> bool {
    const MAX_TICK: f32 = 1.0e7;
    const MIN_KEY: f32 = -256.0;
    const MAX_KEY: f32 = 512.0;
    let key_ok = |key: f32| (MIN_KEY..=MAX_KEY).contains(&key);
    let tick_ok = |tick: f32| tick.abs() <= MAX_TICK;
    model
        .geometry
        .anchors
        .iter()
        .all(|anchor| tick_ok(anchor.pos.0) && key_ok(anchor.pos.1))
        && model.geometry.rect.is_none_or(|rect| {
            tick_ok(rect.tick_lo)
                && tick_ok(rect.tick_hi)
                && key_ok(rect.key_lo)
                && key_ok(rect.key_hi)
        })
}

#[cfg(test)]
mod tests;

mod cursor;
