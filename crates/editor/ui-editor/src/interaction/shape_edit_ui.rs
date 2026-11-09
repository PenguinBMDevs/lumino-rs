//! 020 Custom edit 的 UI 拖拽状态（CP-4.5a）
//!
//! 纯变换在 `editor_state::shape_edit`；本模块负责：
//! 命中手柄 → 拖动预览 → 松手提交。
//!
//! 041C 阶段 3：目标既可以是已 Bake 的 DrawnShape（走 `edit_shape`），
//! 也可以是单支 `PendingComposed`（只改 pending model + 预览音符）。

use lumino_core::shape::ShapeModel;
use lumino_editor_state::editor_state::{
    HandleKind, Mods, Zoom, apply_drag, apply_group_drag, hit_test_box, union_model,
};
use lumino_editor_state::{ComposedKind, PendingComposed, PendingShapeRef};

use crate::Editor;
use crate::interaction::shape_bridge::source_from_model;

mod commit;
mod group;
mod handles;

pub(crate) use handles::ShapeEditHandles;

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
    /// 多选组（仅 ✓ 前 pending 组合；成员见 `Editor::shape_group_edit`）
    Group,
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
    /// 组编辑成员（拖动前模型；单目标为空）
    pub group_parts: Vec<group::GroupPart>,
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
                    | ComposedKind::Single
            )
    }

    /// 进入/退出「编辑图形」模式；进入后隐藏 ShapeSelect 选框。
    pub fn enable_shape_edit(&mut self) -> bool {
        // 已处于编辑态 → 本次为「退出」：丢弃未提交拖拽并恢复选框。
        if self.is_shape_edit_armed() {
            self.exit_shape_edit();
            return true;
        }
        let selected = self.editor_state.shape_select.selection_len();
        // 多选组：仅 ✓ 前 pending；形状/曲线先批量提升为单支组合。
        if selected >= 2 {
            let refs: Vec<PendingShapeRef> = self
                .editor_state
                .shape_select
                .selected_shapes()
                .filter_map(|shape| shape.pending)
                .collect();
            if refs.len() != selected {
                return false;
            }
            let all_composed = refs
                .iter()
                .all(|reference| matches!(reference, PendingShapeRef::Composed(_)));
            let all_plain = refs.iter().all(|reference| {
                matches!(
                    reference,
                    PendingShapeRef::Shape(_) | PendingShapeRef::CurvePath(_)
                )
            });
            if all_plain {
                let Some(indexes) = self.promote_pending_group_for_edit(&refs) else {
                    return false;
                };
                self.shape_edit_armed = Some(ShapeEditTarget::Group);
                self.shape_group_edit = Some(indexes);
                self.grid_cache.clear();
                return true;
            }
            if !all_composed {
                return false;
            }
            let mut indexes = Vec::new();
            for reference in refs {
                let PendingShapeRef::Composed(index) = reference else {
                    return false;
                };
                let Some(composed) = self.editor_state.pending_composed.get(index) else {
                    return false;
                };
                if !Self::pending_handle_supported(composed) {
                    return false;
                }
                if !indexes.contains(&index) {
                    indexes.push(index);
                }
            }
            if indexes.len() < 2 {
                return false;
            }
            self.shape_edit_armed = Some(ShapeEditTarget::Group);
            self.shape_group_edit = Some(indexes);
            self.grid_cache.clear();
            return true;
        }
        if selected != 1 {
            return false;
        }
        self.shape_group_edit = None;
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
            // 020：参数化待确认形状/曲线先提升成单支组合，再按组合编辑（可旋转）
            Some(PendingShapeRef::Shape(_) | PendingShapeRef::CurvePath(_)) => {
                if !self.promote_pending_for_edit() {
                    return false;
                }
                return self.enable_shape_edit();
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
        self.shape_group_edit = None;
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
        let (target, model, original_composed, group_parts) = {
            let Some((target, model)) = self.armed_edit_target() else {
                return false;
            };
            let composed = match target {
                ShapeEditTarget::PendingComposed { .. } | ShapeEditTarget::Group => {
                    self.editor_state.pending_composed.clone()
                }
                ShapeEditTarget::Drawn { .. } => Vec::new(),
            };
            let parts = match target {
                ShapeEditTarget::Group => self.group_parts().unwrap_or_default(),
                _ => Vec::new(),
            };
            (target, model, composed, parts)
        };

        let zoom = Zoom {
            x: self.editor_state.view.zoom_x,
            y: self.editor_state.view.zoom_y,
        };
        let include_center = self.edit_allows_center(target);
        let mods = Mods {
            ctrl: self.ctrl_pressed(),
            shift: self.shift_pressed(),
        };
        // 旋转热区优先于锚点：角外那一圈必须能转，不能被角锚点的 10px 容差吃掉。
        let handle = if self.in_shape_rotate_zone(&model, (tick, key)) {
            HandleKind::Turn
        } else {
            match hit_test_box(&model, (tick, key), zoom, 10.0, include_center) {
                Some(handle) => handle,
                None => return false,
            }
        };
        // DRAW-020：Shift + 边手柄 = 倾斜（按整 key 滑动）；不按 Shift 时仍是单边拉伸。
        let handle = if mods.shift {
            skew_from_side(handle)
        } else {
            handle
        };
        self.shape_edit = Some(ShapeEditState {
            target,
            handle,
            start: (tick, key),
            original: model.clone(),
            original_composed,
            current: model,
            group_parts,
            mods,
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
        if matches!(state.target, ShapeEditTarget::Group) {
            let mut models: Vec<ShapeModel> = state
                .group_parts
                .iter()
                .map(|part| part.model.clone())
                .collect();
            if !apply_group_drag(
                &mut models,
                state.handle,
                state.start,
                (tick, key),
                state.mods,
                k,
            ) {
                self.shape_edit = Some(state);
                return false;
            }
            if models
                .iter()
                .any(|model| !finite_geometry(model) || !sane_geometry(model))
            {
                self.shape_edit = Some(state);
                return false;
            }
            self.write_group_parts(&state.group_parts, &models);
            state.current = union_model(&models).unwrap_or_else(|| state.current.clone());
            self.grid_cache.clear();
            self.shape_edit = Some(state);
            return true;
        }
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
            // 组分支已在上方提前返回
            ShapeEditTarget::Group => {}
        }
        self.grid_cache.clear();
        self.shape_edit = Some(state);
        true
    }

    /// 是否正在拖拽形状手柄
    pub(crate) fn is_shape_editing(&self) -> bool {
        self.shape_edit.is_some()
    }
}

/// Shift + 边手柄 = 倾斜；角点/中心/旋转热区不受影响。
fn skew_from_side(handle: HandleKind) -> HandleKind {
    match handle {
        HandleKind::Side(_) => HandleKind::Skew,
        other => other,
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
#[cfg(test)]
mod tests_ops;

mod cursor;
