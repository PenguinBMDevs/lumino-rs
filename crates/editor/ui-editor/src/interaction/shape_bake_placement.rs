//! DRAW-049：Bake 放置确认态（A+B），复用既有待确认/手柄链路。
//!
//! A：Bake 后把待确认图形推进 `pending_composed`，切到选择工具并自动进入
//!    「编辑图形」——8 个箱体手柄（四角 + 四边 + 中心）全部复用 `shape_edit_ui`。
//! B：在 8 手柄框外空白处拖出新框 → 复用 `EditState::Selecting` 画出框选；
//!    松手按框整体映射 `ShapeModel`，再回到 8 手柄精调。
//!
//! ✓ 仍走唯一提交点 `confirm_shape_tool`；× 直接 `cancel_shape_tool` 丢弃。
//! 本模块不新建手柄数学、不新建预览/按钮，只做状态与箱体变换接线。
//!
//! 归属轨（DRAW-049 阶段 2）见 [`owner`]：放置态只对模型自带 `track` 生效。

mod owner;

use lumino_core::shape::{ShapeModel, ShapeOp};
use lumino_editor_state::editor_state::transform_model_box;
use lumino_editor_state::{
    ComposedKind, DrawnShapeSource, EditState, PendingComposed, PendingShapeRef, ShapeNote,
};
use lumino_message::Tool;
use lumino_ui_core::geom::{Point, Rectangle, Size};

use crate::Editor;

/// 命中/空白判定时围绕 pending 图形外接框的像素余量（与 ShapeSelect 选框同口径）。
const BOX_PADDING_PX: f32 = 3.0;

/// 一次 Bake 放置确认会话。
#[derive(Debug, Clone)]
pub(crate) struct BakePlacementSession {
    /// 在 `pending_composed` 中的下标
    pub pending_index: usize,
    /// ✓ 前原始模型（空白点击复位用）
    pub original_model: ShapeModel,
    /// ✓ 前原始预览音符（空白点击复位用）
    pub original_notes: Vec<ShapeNote>,
    /// 是否正在 B 阶段的拉框
    pub marquee_active: bool,
}

impl Editor {
    /// 是否处于 Bake 放置确认态。
    ///
    /// 除会话存在外还要求 8 手柄仍布防：用户在放置态切选/取消选中后，
    /// 视觉上已经退出编辑；此时必须恢复 pending 音符预览，不能继续隐藏。
    ///
    /// 另外要求当前轨就是会话归属轨：切轨会先收口会话，这里是第二道防线，
    /// 避免任何路径残留「旧轨会话 + 新轨当前」的组合继续接管 ✓ 与手柄。
    pub fn bake_placement_active(&self) -> bool {
        self.bake_session_owner_track() == Some(self.current_track()) && self.is_shape_edit_armed()
    }

    /// 是否正在 B 阶段的拉框。
    pub(crate) fn bake_marquee_active(&self) -> bool {
        self.shape_tools
            .bake_placement
            .as_ref()
            .is_some_and(|session| session.marquee_active)
    }

    /// Bake（形状工具 ✓）→ 进入放置确认态。
    ///
    /// 返回 `false` 时调用方应回退到旧的 `confirm_shape_tool()`：v1 只支持
    /// 单个待确认形状；多形状 / 组合结果暂不接管（沿用原一次性提交）。
    pub(crate) fn begin_shape_bake_placement(&mut self) -> bool {
        if self.shape_tools.bake_placement.is_some() {
            return false;
        }
        if self.editor_state.data.current_track == 0 {
            return false;
        }
        if !self.editor_state.pending_composed.is_empty() {
            return false;
        }
        if self.editor_state.shape_tool.shapes.len() != 1 {
            return false;
        }

        let track = self.editor_state.data.current_track;
        let shape = self.editor_state.shape_tool.shapes[0].clone();
        let source = DrawnShapeSource::Shape {
            kind: shape.kind,
            rect: shape.rect,
            shift_constrained: shape.shift_constrained,
            filled: shape.filled,
            apex_high: shape.apex_high,
        };
        let mut model = self.model_from_source_at_view(&source, track, 1.0);
        // DRAW-010：形状工具的 Mask 不在通用 source 里，和 `confirm_shape_tool`
        // 同一处登记，避免放置态丢遮罩。
        if let Some(mask) = &shape.mask {
            model.ops.push(ShapeOp::Mask(mask.clone()));
        }
        let notes = self.shape_notes_of(&model, track);
        if notes.is_empty() {
            return false;
        }

        // 与 `insert_shape_pending` 同序：先切到选择工具，再推 pending / 建镜像。
        self.set_tool(Tool::ShapeSelect);
        let index = self.editor_state.pending_composed.len();
        self.editor_state
            .pending_composed
            .push(PendingComposed::single(
                ComposedKind::Single,
                model.clone(),
                notes.clone(),
                Vec::new(),
            ));
        self.shape_tools.pending_composed_redo.clear();
        self.shape_tools.pending_edit_undo.clear();
        self.shape_tools.pending_edit_redo.clear();
        // 先清空形状工具的待确认列表，再重建镜像：否则 `rebuild_pending_mirrors`
        // 会为刚转交给 pending 的形状再建一份失效镜像（指向空容器）。
        self.editor_state.shape_tool.clear_pending();
        self.editor_state.shape_select.remove_pending_all();
        self.rebuild_pending_mirrors();
        if let Some(id) = self
            .editor_state
            .shape_select
            .pending_mirror(PendingShapeRef::Composed(index))
        {
            self.editor_state.shape_select.select_only(id);
        }
        self.shape_tools.bake_placement = Some(BakePlacementSession {
            pending_index: index,
            original_model: model,
            original_notes: notes,
            marquee_active: false,
        });
        // 清掉可能残留的旧编辑目标，再自动进入 8 手柄编辑；失败时仍可用既有
        // 「编辑图形」入口。
        self.shape_tools.edit = None;
        self.shape_tools.edit_armed = None;
        self.shape_tools.group_edit = None;
        let _ = self.enable_shape_edit();
        self.mark_notes_changed();
        true
    }

    /// 选择工具按下时先尝试接管：8 手柄命中 → 交给 shape_edit_ui；
    /// 手柄框外空白 → 进入 B 阶段拉框。返回 `true` 表示已消费。
    pub(crate) fn bake_placement_press(&mut self, pos: Point, snapped_tick: f32, key: u16) -> bool {
        if !self.bake_placement_active() || self.editor_state.current_tool() != Tool::ShapeSelect {
            return false;
        }
        let tick = self.pos_to_tick(pos);
        let raw_key = self.pos_to_raw_key(pos);
        if self.begin_shape_edit(tick, raw_key) {
            return true;
        }
        // 框内但未命中手柄：交给既有 ShapeSelect 选中/拖动逻辑。
        if pending_screen_box(self).is_some_and(|rect| rect.contains(pos)) {
            return false;
        }
        begin_bake_marquee(self, snapped_tick, key);
        true
    }

    /// B 阶段拉框移动。
    pub(crate) fn handle_bake_marquee_moved(&mut self, pos: Point) {
        if !self.bake_marquee_active() {
            return;
        }
        let tick = self.pos_to_tick(pos);
        let key = self.pos_to_raw_key(pos).round().clamp(0.0, 255.0) as u16;
        let (_, current_tick) = match self.selection_ui.marquee_anchor() {
            Some(anchor) => self.marquee_edges(anchor, tick),
            None => (tick, tick),
        };
        let key_y = self.key_to_y(key);
        if let EditState::Selecting {
            current_tick: cur_tick,
            current_key,
            current_y,
            ..
        } = &mut self.editor_state.interaction.edit_state
        {
            *cur_tick = current_tick;
            *current_key = key;
            *current_y = key_y;
        }
    }

    /// B 阶段拉框松手：有效框 → 映射模型；点击（无效框）→ 复位到原始模型。
    pub(crate) fn handle_bake_marquee_released(&mut self, edit_state: EditState) {
        let Some(session) = self.shape_tools.bake_placement.as_mut() else {
            return;
        };
        session.marquee_active = false;
        let index = session.pending_index;
        let new_box = match edit_state {
            EditState::Selecting {
                start_tick,
                current_tick,
                start_key,
                current_key,
                ..
            } => {
                let (t0, t1) = (start_tick.min(current_tick), start_tick.max(current_tick));
                let (k0, k1) = (
                    start_key.min(current_key) as f32,
                    start_key.max(current_key) as f32,
                );
                (t1 - t0 >= 1.0 && k1 - k0 >= 1.0).then_some((t0, k0, t1, k1))
            }
            _ => None,
        };
        if let Some(new_box) = new_box {
            apply_bake_box(self, index, new_box);
        } else {
            restore_bake_original(self, index);
        }
        self.mark_notes_changed();
        if !self.is_shape_edit_armed() {
            let _ = self.enable_shape_edit();
        }
    }

    /// 放置态 ✓：走唯一提交点；成功后结束会话。
    ///
    /// 归属轨守卫是防御性的：正常路由已被 `bake_placement_active()` 拦在门外，
    /// 但任何新调用方都不该能绕过「只在归属轨提交」这条契约。
    pub(crate) fn confirm_bake_placement(&mut self) -> bool {
        if self.bake_session_owner_track() != Some(self.current_track()) {
            return false;
        }
        let ok = self.confirm_shape_tool();
        if ok {
            self.shape_tools.bake_placement = None;
        }
        ok
    }

    /// 从已存在的 pending 重新进入放置态（第二次撤销回到 pending 后，再点 ✓
    /// 应回到选择/8 手柄，而不是直接 Bake）。
    pub(crate) fn enter_bake_placement_from_pending(&mut self) -> bool {
        // DRAW-049 A：只有当前轨就是 pending 归属轨才允许进入放置态；
        // 否则切轨后按 ✓ 会把旧轨模型带进新轨。必须在重建镜像/选中之前返回。
        if self.pending_single_owner_track() != Some(self.current_track()) {
            return false;
        }
        if self.shape_tools.bake_placement.is_some() {
            return false;
        }
        if self.editor_state.pending_composed.len() != 1 {
            return false;
        }
        let index = 0;
        let Some(composed) = self.editor_state.pending_composed.get(index) else {
            return false;
        };
        if composed.kind != ComposedKind::Single || composed.results.len() != 1 {
            return false;
        }
        let Some(part) = composed.results.first() else {
            return false;
        };
        let model = part.model.clone();
        let notes = part.notes.clone();

        if self.editor_state.current_tool() != Tool::ShapeSelect {
            self.set_tool(Tool::ShapeSelect);
        }
        self.rebuild_pending_mirrors();
        if let Some(id) = self
            .editor_state
            .shape_select
            .pending_mirror(PendingShapeRef::Composed(index))
        {
            self.editor_state.shape_select.select_only(id);
        }
        self.shape_tools.edit = None;
        self.shape_tools.edit_armed = None;
        self.shape_tools.group_edit = None;
        self.shape_tools.bake_placement = Some(BakePlacementSession {
            pending_index: index,
            original_model: model,
            original_notes: notes,
            marquee_active: false,
        });
        let _ = self.enable_shape_edit();
        self.mark_notes_changed();
        true
    }

    /// 放置态 ×：直接丢弃整个 pending，不写文档/历史。
    pub(crate) fn cancel_bake_placement(&mut self) {
        self.shape_tools.bake_placement = None;
        self.cancel_shape_tool();
    }

    /// 撤销放置态但保留 pending：第二次 Ctrl+Z 回到 pending 图形/音符预览。
    pub(crate) fn exit_bake_placement_to_pending(&mut self) {
        self.shape_tools.bake_placement = None;
        self.shape_tools.edit = None;
        self.shape_tools.edit_armed = None;
        self.shape_tools.group_edit = None;
        // 放置态退出后不再保留“黄色选择框”：pending 仍在，但未选中；
        // 避免留下一个不可操作的残留框（音符预览由 shape_preview 负责）。
        self.editor_state.shape_select.clear_selection();
    }
}

/// pending 图形在画布局部坐标下的外接框（含少量 padding）。
fn pending_screen_box(editor: &Editor) -> Option<Rectangle> {
    let session = editor.shape_tools.bake_placement.as_ref()?;
    let part = editor
        .editor_state
        .pending_composed
        .get(session.pending_index)?
        .results
        .first()?;
    let (x0, y0, x1, y1) = part.model.geometry.bounds()?;
    let a = editor.line_pos_screen_pos((x0, y0));
    let b = editor.line_pos_screen_pos((x1, y1));
    let left = a.x.min(b.x) - BOX_PADDING_PX;
    let top = a.y.min(b.y) - BOX_PADDING_PX;
    let width = (a.x - b.x).abs() + BOX_PADDING_PX * 2.0;
    let height = (a.y - b.y).abs() + BOX_PADDING_PX * 2.0;
    Some(Rectangle::new(
        Point::new(left, top),
        Size::new(width.max(1.0), height.max(1.0)),
    ))
}

/// 进入 B 阶段：复用二维框选（`EditState::Selecting`）。
fn begin_bake_marquee(editor: &mut Editor, snapped_tick: f32, key: u16) {
    let (start_tick, current_tick) = editor.begin_marquee_ticks(snapped_tick);
    let key_y = editor.key_to_y(key);
    editor.editor_state.interaction.edit_state = EditState::Selecting {
        start_tick,
        current_tick,
        start_key: key,
        current_key: key,
        start_y: key_y,
        current_y: key_y,
    };
    if let Some(session) = &mut editor.shape_tools.bake_placement {
        session.marquee_active = true;
    }
}

/// 把当前 pending 模型按框映射，并重生成预览音符。
fn apply_bake_box(editor: &mut Editor, index: usize, new_box: (f32, f32, f32, f32)) {
    let Some(current) = editor
        .editor_state
        .pending_composed
        .get(index)
        .and_then(|composed| composed.results.first())
        .map(|part| part.model.clone())
    else {
        return;
    };
    let Some(from) = current.geometry.bounds() else {
        return;
    };
    let mut model = current;
    transform_model_box(&mut model, from, new_box);
    let track = model.track as usize;
    let notes = editor.shape_notes_of(&model, track);
    if let Some(part) = editor
        .editor_state
        .pending_composed
        .get_mut(index)
        .and_then(|composed| composed.results.first_mut())
    {
        part.model = model;
        part.notes = notes;
    }
    editor.refresh_pending_mirror_geometry();
}

/// 空白点击：恢复 Bake 前的模型与预览音符。
fn restore_bake_original(editor: &mut Editor, index: usize) {
    let Some(session) = editor.shape_tools.bake_placement.as_ref() else {
        return;
    };
    let model = session.original_model.clone();
    let notes = session.original_notes.clone();
    if let Some(part) = editor
        .editor_state
        .pending_composed
        .get_mut(index)
        .and_then(|composed| composed.results.first_mut())
    {
        part.model = model;
        part.notes = notes;
    }
    editor.refresh_pending_mirror_geometry();
}

#[cfg(test)]
mod tests;
