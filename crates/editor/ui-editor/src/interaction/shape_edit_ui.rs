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
    HandleKind, Mods, Zoom, apply_drag, box_handles, hit_test_box,
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

    /// 进入/退出「编辑图形」模式；进入后隐藏 ShapeSelect 选框。
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
        // 已处于编辑态 → 本次为「退出」：丢弃未提交拖拽并恢复选框。
        if self.is_shape_edit_armed() {
            self.exit_shape_edit();
            return true;
        }
        self.shape_edit_armed = Some(target);
        self.grid_cache.clear();
        true
    }

    /// 是否处于「编辑图形」态。
    ///
    /// 以**当前选中目标是否仍匹配**为准：换选/取消选中后自动失效，
    /// 不会出现「选框被隐藏、锚点却不在」的空窗。
    pub fn is_shape_edit_armed(&self) -> bool {
        self.armed_edit_target().is_some()
    }

    /// 退出「编辑图形」：丢弃未提交的拖拽，恢复 ShapeSelect 选框。
    ///
    /// 入口：双击空白处、右键「编辑图形」再点一次、切换工具。
    pub fn exit_shape_edit(&mut self) -> bool {
        let was = self.shape_edit_armed.is_some() || self.shape_edit.is_some();
        if self.shape_edit.is_some() {
            self.cancel_shape_edit();
        }
        self.shape_edit_armed = None;
        self.grid_cache.clear();
        was
    }

    /// 编辑态命中：8 点箱体（pending 额外中心）+ 角外 32px 旋转区。
    ///
    /// 未进入「编辑图形」时一律不接管：锚点只是示意，不会和 ShapeSelect 选框抢点击。
    pub(crate) fn begin_shape_edit(&mut self, tick: f32, key: f32) -> bool {
        if self.current_tool() != lumino_message::Tool::ShapeSelect {
            return false;
        }
        let (target, model, original_composed) = {
            let Some((target, model)) = self.armed_edit_target() else {
                return false;
            };
            let model = model.clone();
            let composed = match target {
                ShapeEditTarget::PendingComposed { .. } => {
                    self.editor_state.pending_composed.clone()
                }
                ShapeEditTarget::Drawn { .. } => Vec::new(),
            };
            (target, model, composed)
        };

        let zoom = Zoom {
            x: self.editor_state.view.zoom_x,
            y: self.editor_state.view.zoom_y,
        };
        let include_center = self.edit_allows_center(target);
        // 旋转热区优先于锚点：角外那一圈必须能转，不能被角锚点的 10px 容差吃掉。
        let handle = if self.in_shape_rotate_zone(&model, (tick, key)) {
            HandleKind::Turn
        } else {
            match hit_test_box(&model, (tick, key), zoom, 10.0, include_center) {
                Some(handle) => handle,
                None => return false,
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
            // 旋转比例必须是**屏幕像素**意义上的「1 key / 1 tick」，横纵卷帘都成立。
            // 之前用 `model.k * ppq`（数据单位）会把它当成像素比例，屏幕上看就是
            // 旋转被拉成极端椭圆，甚至触发几何守卫整帧丢弃 —— 表现为「旋转没实现」。
            HandleKind::Turn => self.turn_aspect(),
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

    /// 取消拖拽并恢复原几何（Esc / 退出编辑图形）
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

    /// 编辑态锚点（屏幕坐标）：8 个箱体锚点 + 可选中心锚点。
    pub(crate) fn shape_edit_handles(&self) -> ShapeEditHandles {
        let Some((target, model)) = self.armed_edit_target() else {
            return ShapeEditHandles::default();
        };
        let zoom = Zoom {
            x: self.editor_state.view.zoom_x,
            y: self.editor_state.view.zoom_y,
        };
        let include_center = self.edit_allows_center(target);
        let Some(layout) = box_handles(model, zoom, include_center) else {
            return ShapeEditHandles::default();
        };
        let (center, box_kinds): (Vec<_>, Vec<_>) = layout
            .handles
            .into_iter()
            .partition(|(kind, _)| matches!(kind, HandleKind::Move));
        ShapeEditHandles {
            box_points: box_kinds
                .into_iter()
                .map(|(_, pos)| self.line_pos_screen_pos(pos))
                .collect(),
            center: center
                .first()
                .map(|(_, pos)| self.line_pos_screen_pos(*pos)),
        }
    }

    /// 是否正在拖拽形状手柄
    pub(crate) fn is_shape_editing(&self) -> bool {
        self.shape_edit.is_some()
    }
}

/// 编辑态锚点屏幕位置：8 个箱体锚点 + 可选中心锚点。
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct ShapeEditHandles {
    /// 4 角 + 4 边中点
    pub box_points: Vec<Point>,
    /// 中心锚点（仅未 Bake 组合）
    pub center: Option<Point>,
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
#[cfg(test)]
mod tests_ops;

mod cursor;
