//! 选择框边界计算：get_selection_box_rects + get_selection_box_bounds + hit_test_selection_box
//!
//! 三层路径优先级：
//! 1. 非 ghost 路径：O(1) 读取 `selected_bounds` 缓存
//! 2. ghost 路径：O(1) 缓存 + delta（拖拽中或松手后待提交）
//! 3. O(N) 回退路径：缓存失效兜底，同时恢复缓存供后续帧走 O(1)
//!
//! **复制模式单框**：复制（`pending_copy` / `DraggingSelectionCopy`）时返回
//! **副本框**（选中音符 + 复制 delta）——**只保留最新件（副本）的框选**，
//! 原件不再框选（用户要求）；非复制模式返回选中音符的单框。

use lumino_ui_core::geom::Point;

use super::super::{EditState, Editor, SelectionHitType};
use super::Bounds;
use crate::rendering::{copy_deltas_for_index, is_copy_ghosted};
use lumino_editor_state::{DragState, editor_state::hit_test};

/// 屏幕坐标矩形 (min_x, max_x, min_y, max_y)
type ScreenRect = (f32, f32, f32, f32);

/// 拖拽 / 复制 ghost 状态：delta 与活跃标志。
#[derive(Clone, Copy, Debug)]
struct GhostState {
    /// 移动 delta（pending + 当前 dragging 叠加）
    move_delta: (i64, i16),
    /// 复制 delta（pending_copy + 当前 copy dragging 叠加）
    copy_delta: (i64, i16),
    /// 移动 ghost 是否活跃（pending 或 Dragging / DraggingSelection）
    move_active: bool,
    /// 复制 ghost 是否活跃（pending_copy 或 DraggingSelectionCopy）
    copy_active: bool,
}

impl GhostState {
    fn capture(
        edit_state: &EditState,
        pending: Option<&DragState>,
        pending_copy: Option<&DragState>,
    ) -> Self {
        // 初始值：pending 的 delta（已松手但未提交）
        let (mut move_tick, mut move_key) = pending
            .map(|p| (p.delta_tick, p.delta_key))
            .unwrap_or((0, 0));
        let (mut copy_tick, mut copy_key) = pending_copy
            .map(|p| (p.delta_tick, p.delta_key))
            .unwrap_or((0, 0));
        let mut dragging = false;
        match edit_state {
            // 叠加当前 drag_state 的 delta（Dragging 或 DraggingSelection）
            EditState::Dragging { drag_state, .. }
            | EditState::DraggingSelection { drag_state } => {
                move_tick = move_tick.saturating_add(drag_state.delta_tick);
                move_key = move_key.saturating_add(drag_state.delta_key);
                dragging = true;
            }
            EditState::DraggingSelectionCopy { drag_state } => {
                copy_tick = copy_tick.saturating_add(drag_state.delta_tick);
                copy_key = copy_key.saturating_add(drag_state.delta_key);
            }
            _ => {}
        }
        Self {
            move_delta: (move_tick, move_key),
            copy_delta: (copy_tick, copy_key),
            move_active: pending.is_some() || dragging,
            copy_active: pending_copy.is_some()
                || matches!(edit_state, EditState::DraggingSelectionCopy { .. }),
        }
    }

    fn needs_ghost(self) -> bool {
        self.move_active || self.copy_active
    }

    /// 复制模式只保留副本框：副本 = 移动 + 复制 delta；否则只跟移动。
    fn visible_delta(self) -> (i64, i16) {
        if self.copy_active {
            (
                self.move_delta.0.saturating_add(self.copy_delta.0),
                self.move_delta.1.saturating_add(self.copy_delta.1),
            )
        } else {
            self.move_delta
        }
    }
}

impl Editor {
    /// 选择框矩形集合（屏幕坐标）
    ///
    /// - 复制模式（`pending_copy` / `DraggingSelectionCopy`）：只返回**副本框**
    ///   （移动 + 复制 delta）——只保留最新件（副本）的框选，原件不再框选
    /// - 其他状态：返回单个框（选中音符的包围盒）
    /// - 无选中：返回空 Vec
    ///
    /// 渲染与命中测试请使用本方法；`get_selection_box_bounds` 为兼容入口
    /// （返回所有框的并集）。
    pub fn get_selection_box_rects(&self) -> Vec<ScreenRect> {
        puffin::profile_function!();

        let selected = &self.editor_state.interaction.selected_notes;
        if selected.is_empty() {
            return Vec::new();
        }
        let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);
        let ghost = GhostState::capture(
            &self.editor_state.interaction.edit_state,
            self.transient.pending_drag.as_ref(),
            self.transient.pending_copy.as_ref(),
        );

        // 缓存有效：非 ghost 直接取原始框；ghost 在缓存上叠加 delta（O(1)）
        if let Some((min_t, max_te, max_k, min_k)) = self.selection_ui.bounds.get() {
            if !ghost.needs_ghost() {
                return vec![bounds_to_screen_rect(self, min_t, max_te, max_k, min_k)];
            }
            puffin::profile_scope!("get_selection_box_rects::ghost_o1");
            let (dt, dk) = ghost.visible_delta();
            let (g_min_t, g_max_te, g_max_k, g_min_k) =
                ghost_rect(min_t, max_te, max_k, min_k, dt, dk, max_key);
            // 不缓存 ghost 结果（delta 每帧变化）
            return vec![bounds_to_screen_rect(
                self, g_min_t, g_max_te, g_max_k, g_min_k,
            )];
        }

        // 缓存失效（理论上不应发生）：O(N) 回退并恢复缓存
        if ghost.needs_ghost() {
            selection_rects_from_ghost_scan(self, &ghost)
        } else {
            selection_rects_from_plain_scan(self)
        }
    }

    /// 兼容入口：返回所有选择框的并集（覆盖所有选中与副本）
    ///
    /// 语义与上一代 `get_selection_box_bounds` 一致；复制模式的副本框（最新件
    /// 框选）请使用 `get_selection_box_rects`。
    pub fn get_selection_box_bounds(&self) -> Option<ScreenRect> {
        let mut rects = self.get_selection_box_rects().into_iter();
        let first = rects.next()?;
        let (min_x, max_x, min_y, max_y) = rects
            .fold(first, |(a1, a2, a3, a4), (b1, b2, b3, b4)| {
                (a1.min(b1), a2.max(b2), a3.min(b3), a4.max(b4))
            });
        Some((min_x, max_x, min_y, max_y))
    }

    /// 命中检测：遍历所有选择框（复制模式 = 副本框），返回第一个命中
    pub fn hit_test_selection_box(&self, pos: Point) -> Option<SelectionHitType> {
        for bounds in self.get_selection_box_rects() {
            if let Some(hit) = hit_test::hit_test_selection_box(bounds, (pos.x, pos.y)) {
                return Some(hit);
            }
        }
        None
    }
}

/// 非 ghost 回退：全量扫描选中集，重建缓存并返回单框。
fn selection_rects_from_plain_scan(editor: &Editor) -> Vec<ScreenRect> {
    puffin::profile_scope!("get_selection_box_rects::fallback");
    let mut bounds = Bounds::default();
    for i in editor.editor_state.interaction.selected_notes.iter() {
        if let Some(n) = editor.editor_state.data.get_note_view(i) {
            bounds.add(n.tick, n.tick + n.length, n.key);
        }
    }
    let raw = bounds.get();
    editor.selection_ui.bounds.set(raw);
    raw.map(|(min_t, max_te, max_k, min_k)| {
        vec![bounds_to_screen_rect(editor, min_t, max_te, max_k, min_k)]
    })
    .unwrap_or_default()
}

/// ghost 回退：单遍扫描同时累计原始包围盒（恢复缓存）与可见包围盒。
///
/// 使用 `get_note_view(idx)` 替代 `notes.get(idx)`，在 NoteStore 启用时零 clone
/// （16M 音符场景下，B-tree 遍历每个节点 clone Note → SoA 数组直接取 NoteView）。
fn selection_rects_from_ghost_scan(editor: &Editor, ghost: &GhostState) -> Vec<ScreenRect> {
    puffin::profile_scope!("get_selection_box_rects::ghost_on");
    let data = &editor.editor_state.data;
    let selected = &editor.editor_state.interaction.selected_notes;
    let max_key = editor.editor_state.view.visible_key_count.saturating_sub(1);
    let edit_state = &editor.editor_state.interaction.edit_state;
    let pending = &editor.transient.pending_drag;
    let pending_copy = &editor.transient.pending_copy;

    // 逐音符 drag delta：pending 只对 pending.selected 里的音符叠加
    let (drag_dt, drag_dk) = match edit_state {
        EditState::Dragging { drag_state, .. } | EditState::DraggingSelection { drag_state } => {
            (drag_state.delta_tick, drag_state.delta_key)
        }
        _ => (0i64, 0i16),
    };

    let mut raw = Bounds::default();
    let mut visible = Bounds::default();
    for i in selected.iter() {
        let Some(n) = data.get_note_view(i) else {
            continue;
        };
        // raw bounds（无 delta）— 用于恢复缓存
        raw.add(n.tick, n.tick + n.length, n.key);
        if ghost.copy_active {
            // 副本 ghost = note + (移动 + 复制) delta；
            // 连续复制（pending_copy + DraggingSelectionCopy）时旧副本与新
            // 副本并存，两条都计入副本框边界。
            if is_copy_ghosted(i, pending_copy, edit_state) {
                let (old_copy, new_copy) =
                    copy_deltas_for_index(i, pending_copy, pending, edit_state);
                for (cdt, cdk) in [old_copy, new_copy].into_iter().flatten() {
                    let tick = (n.tick + cdt as f32).max(0.0);
                    let key = (n.key as i32 + cdk as i32).clamp(0, max_key as i32) as u16;
                    visible.add(tick, tick + n.length, key);
                }
            }
        } else if ghost.move_active {
            // 移动 ghost bounds（原件 + 移动 delta）
            let mut dt = drag_dt;
            let mut dk = drag_dk;
            if let Some(pending) = pending
                && i < pending.selected.len()
                && pending.selected[i]
            {
                dt = dt.saturating_add(pending.delta_tick);
                dk = dk.saturating_add(pending.delta_key);
            }
            let tick = (n.tick + dt as f32).max(0.0);
            let key = (n.key as i32 + dk as i32).clamp(0, max_key as i32) as u16;
            visible.add(tick, tick + n.length, key);
        }
    }

    // 恢复 raw bounds 缓存，后续帧走 O(1) ghost 路径
    editor.selection_ui.bounds.set(raw.get());

    // 复制模式只保留最新件（副本）框选；移动模式跟随 ghost
    visible
        .get()
        .map(|(min_t, max_te, max_k, min_k)| {
            vec![bounds_to_screen_rect(editor, min_t, max_te, max_k, min_k)]
        })
        .unwrap_or_default()
}

#[inline]
fn bounds_to_screen_rect(
    editor: &Editor,
    min_t: f32,
    max_te: f32,
    max_k: u16,
    min_k: u16,
) -> ScreenRect {
    if editor.editor_state.is_vertical_roll {
        vertical_rect_from_bounds(
            &editor.editor_state.view,
            editor.editor_state.canvas.size_y,
            min_t,
            max_te,
            max_k,
            min_k,
        )
    } else {
        rect_from_bounds(&editor.editor_state.view, min_t, max_te, max_k, min_k)
    }
}

/// (min_t, max_te, max_k, min_k) → 屏幕坐标矩形（纵向转置）
#[inline]
fn rect_from_bounds(
    view: &lumino_core::view_state::ViewState,
    min_t: f32,
    max_te: f32,
    max_k: u16,
    min_k: u16,
) -> ScreenRect {
    // 注意：此函数仅在横向路径使用；纵向由 Editor::vertical_rect_from_bounds 处理
    (
        view.tick_to_x(min_t),
        view.tick_to_x(max_te),
        view.key_to_y(max_k),
        view.key_to_y(min_k) + view.zoom_y,
    )
}

/// 纵向 (min_t, max_te, max_k, min_k) → 屏幕坐标矩形
#[inline]
fn vertical_rect_from_bounds(
    view: &lumino_core::view_state::ViewState,
    canvas_h: f32,
    min_t: f32,
    max_te: f32,
    max_k: u16,
    min_k: u16,
) -> ScreenRect {
    let kb_h = view.keyboard_width;
    let grid_bottom = canvas_h - kb_h;
    // X：key 水平
    let min_x = min_k as f32 * view.zoom_y - view.scroll_y;
    let max_x = max_k as f32 * view.zoom_y - view.scroll_y + view.zoom_y;
    // Y：tick 垂直（头部在底部，向上递增）
    let min_y = grid_bottom - max_te * view.zoom_x + view.scroll_x;
    let max_y = grid_bottom - min_t * view.zoom_x + view.scroll_x;
    (min_x, max_x, min_y.min(max_y), min_y.max(max_y))
}

/// 对 (min_t, max_te, max_k, min_k) 边界应用 ghost delta（tick 平移 + key clamp）
#[inline]
fn ghost_rect(
    min_t: f32,
    max_te: f32,
    max_k: u16,
    min_k: u16,
    dt: i64,
    dk: i16,
    max_key: u16,
) -> (f32, f32, u16, u16) {
    (
        (min_t + dt as f32).max(0.0),
        max_te + dt as f32,
        (max_k as i32 + dk as i32).clamp(0, max_key as i32) as u16,
        (min_k as i32 + dk as i32).clamp(0, max_key as i32) as u16,
    )
}
