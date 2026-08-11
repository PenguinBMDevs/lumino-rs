//! 选择框边界计算：get_selection_box_bounds + hit_test_selection_box
//!
//! 三层路径优先级：
//! 1. 非 ghost 路径：O(1) 读取 `selected_bounds` 缓存
//! 2. ghost 路径：O(1) 缓存 + delta（拖拽中或松手后待提交）
//! 3. O(N) 回退路径：缓存失效兜底，同时恢复缓存供后续帧走 O(1)

use iced_core::Point;

use super::super::{EditState, Editor, SelectionHitType};
use crate::rendering::{copy_delta_for_index, is_copy_ghosted};
use lumino_editor_state::editor_state::hit_test;

impl Editor {
    pub fn get_selection_box_bounds(&self) -> Option<(f32, f32, f32, f32)> {
        puffin::profile_function!();

        let data = &self.editor_state.data;
        let view = &self.editor_state.view;
        let selected = &self.editor_state.interaction.selected_notes;
        let max_key = view.visible_key_count.saturating_sub(1);
        let edit_state = &self.editor_state.interaction.edit_state;
        let pending = &self.pending_drag_state;
        let pending_copy = &self.pending_copy_drag_state;

        let has_selection_bitset = self.editor_state.interaction.selection_bitset.is_some();
        if selected.is_empty() && !has_selection_bitset {
            return None;
        }

        // 判断是否需要 ghost delta（拖拽中或松手后待提交状态）
        // 注意：DraggingSelection 也需要 ghost，否则 selection_box 不跟随拖动
        // 复制模式（DraggingSelectionCopy / pending_copy）同样需要：选择框要
        // 覆盖副本位置（union 原件 ∪ 副本），否则副本没有"框选"视觉反馈
        let needs_ghost = pending.is_some()
            || pending_copy.is_some()
            || matches!(
                edit_state,
                EditState::Dragging { .. }
                    | EditState::DraggingSelection { .. }
                    | EditState::DraggingSelectionCopy { .. }
            );

        // 非 ghost 路径：使用增量维护的 selected_bounds，O(1)
        if !needs_ghost && let Some((min_t, max_te, max_k, min_k)) = self.selected_bounds.get() {
            return Some((
                view.tick_to_x(min_t),
                view.tick_to_x(max_te),
                view.key_to_y(max_k),
                view.key_to_y(min_k) + view.zoom_y,
            ));
        }
        // 缓存失效时回退到全量扫描（理论上不应发生，兜底）

        // ═══ ghost 路径：O(1) 快速路径 ═══
        // 所有选中音符共享同一 delta（无论是 drag_state 的 delta 还是 pending 的 delta），
        // 直接从 selected_bounds 缓存 + 应用 delta，避免 O(N) 遍历 1600W 音符。
        //
        // 正确性依据：
        // - f(t) = max(0, t + dt) 单调递增 → min_t_ghost = f(min_t_original)
        // - h(k) = clamp(k + dk, 0, max_key) 单调递增 → min/max_key_ghost = h(min/max_key_original)
        // - max_te_ghost = max_te_original + dt（t+dt>=0 时精确，否则近似）
        //
        // 注意：pending 非空时（松手后待提交），delta 来源为 pending.delta_tick/delta_key；
        // 否则从 edit_state 中获取。之前的代码用 pending.is_none() 阻塞了 O(1) 路径，
        // 导致松手后每帧走 O(N) 的 ghost_on 回退（5.2s/帧 × 6帧 = 31s）。
        if needs_ghost && let Some((min_t, max_te, max_k, min_k)) = self.selected_bounds.get() {
            puffin::profile_scope!("get_selection_box_bounds::ghost_o1");
            // 移动 delta：pending_drag + 当前 Dragging/DraggingSelection drag
            // 当 pending 存在时（已松手但未提交），当前 drag_state 的 delta 仍需叠加，
            // 否则第二次拖动时选择框不跟随鼠标移动。
            let (move_dt, move_dk) = {
                // 初始值：从 pending 获取（如果有）
                let (mut dt, mut dk) = if let Some(pending) = pending {
                    (pending.delta_tick, pending.delta_key)
                } else {
                    (0i64, 0i16)
                };
                // 叠加当前 drag_state 的 delta（Dragging 或 DraggingSelection）
                match edit_state {
                    EditState::Dragging { drag_state, .. }
                    | EditState::DraggingSelection { drag_state } => {
                        dt = dt.saturating_add(drag_state.delta_tick);
                        dk = dk.saturating_add(drag_state.delta_key);
                    }
                    _ => {}
                }
                (dt, dk)
            };
            // 复制 delta：pending_copy + DraggingSelectionCopy drag
            let (copy_dt, copy_dk) = {
                let (mut dt, mut dk) = if let Some(copy) = pending_copy {
                    (copy.delta_tick, copy.delta_key)
                } else {
                    (0i64, 0i16)
                };
                if let EditState::DraggingSelectionCopy { drag_state } = edit_state {
                    dt = dt.saturating_add(drag_state.delta_tick);
                    dk = dk.saturating_add(drag_state.delta_key);
                }
                (dt, dk)
            };

            let move_active = pending.is_some()
                || matches!(
                    edit_state,
                    EditState::Dragging { .. } | EditState::DraggingSelection { .. }
                );
            let copy_active = pending_copy.is_some()
                || matches!(edit_state, EditState::DraggingSelectionCopy { .. });

            // 复制模式：**独立框选**——选择框只覆盖副本位置
            // （原件 + 移动 delta + 复制 delta），不包含原件位置。
            // 原件保持选中（selected_notes 不动，内存层一致），
            // 副本拥有自己的框选，可从副本位置继续 Ctrl+拖动复制。
            if copy_active {
                let (dt, dk) = (
                    move_dt.saturating_add(copy_dt),
                    move_dk.saturating_add(copy_dk),
                );
                let (g_min_t, g_max_te, g_max_k, g_min_k) =
                    ghost_rect(min_t, max_te, max_k, min_k, dt, dk, max_key);
                // 不缓存 ghost 结果（delta 每帧变化）
                return Some((
                    view.tick_to_x(g_min_t),
                    view.tick_to_x(g_max_te),
                    view.key_to_y(g_max_k),
                    view.key_to_y(g_min_k) + view.zoom_y,
                ));
            }
            // 移动模式（无复制）：选择框跟随移动 ghost（原件 + move delta）
            if move_active {
                let (g_min_t, g_max_te, g_max_k, g_min_k) =
                    ghost_rect(min_t, max_te, max_k, min_k, move_dt, move_dk, max_key);
                // 不缓存 ghost 结果（delta 每帧变化）
                return Some((
                    view.tick_to_x(g_min_t),
                    view.tick_to_x(g_max_te),
                    view.key_to_y(g_max_k),
                    view.key_to_y(g_min_k) + view.zoom_y,
                ));
            }
            // needs_ghost 为 true 时必有 move 或 copy 活跃，此处理论不可达
        }
        // 缓存失效时回退到 O(N) 计算（理论上不应发生，兜底）

        // ═══ O(N) 回退路径 ═══
        // 场景：selected_bounds 缓存失效（理论上不应发生，兜底）
        //
        // 性能优化：O(N) 回退中同时计算 raw_bounds（无 delta）并恢复 selected_bounds 缓存，
        // 确保后续帧走 O(1) ghost 路径，避免每帧都 O(N) 扫描 1600W 选中音符。
        //
        // 使用 get_note_view(idx) 替代 notes.get(idx)，在 NoteStore 启用时零 clone
        // （16M 音符场景下，B-tree 遍历每个节点 clone Note → SoA 数组直接取 NoteView）。
        let mut min_t = f32::INFINITY;
        let mut max_te = f32::NEG_INFINITY;
        let mut max_k = u16::MIN;
        let mut min_k = u16::MAX;
        let mut any = false;

        if needs_ghost {
            puffin::profile_scope!("get_selection_box_bounds::ghost_on");
            let (drag_dt, drag_dk) = match edit_state {
                EditState::Dragging { drag_state, .. }
                | EditState::DraggingSelection { drag_state } => {
                    (drag_state.delta_tick, drag_state.delta_key)
                }
                _ => (0i64, 0i16),
            };

            // 同时计算 raw_bounds（用于恢复缓存）和渲染 bounds（用于返回结果）
            let mut raw_min_t = f32::INFINITY;
            let mut raw_max_te = f32::NEG_INFINITY;
            let mut raw_max_k = u16::MIN;
            let mut raw_min_k = u16::MAX;

            let move_active = pending.is_some()
                || matches!(
                    edit_state,
                    EditState::Dragging { .. } | EditState::DraggingSelection { .. }
                );
            let copy_active = pending_copy.is_some()
                || matches!(edit_state, EditState::DraggingSelectionCopy { .. });

            for &i in selected.iter() {
                let Some(n) = data.get_note_view(i) else {
                    continue;
                };
                any = true;
                // raw bounds（无 delta）— 用于恢复缓存
                raw_min_t = raw_min_t.min(n.tick);
                raw_max_te = raw_max_te.max(n.tick + n.length);
                raw_max_k = raw_max_k.max(n.key);
                raw_min_k = raw_min_k.min(n.key);
                if copy_active {
                    // 独立框选：只算副本位置（原件 + 移动 delta + 复制 delta）
                    if is_copy_ghosted(i, pending_copy, edit_state)
                        && let Some((cdt, cdk)) =
                            copy_delta_for_index(i, pending_copy, pending, edit_state)
                    {
                        let tick = (n.tick + cdt as f32).max(0.0);
                        let key = (n.key as i32 + cdk as i32).clamp(0, max_key as i32) as u16;
                        min_t = min_t.min(tick);
                        max_te = max_te.max(tick + n.length);
                        max_k = max_k.max(key);
                        min_k = min_k.min(key);
                    }
                } else if move_active {
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
                    min_t = min_t.min(tick);
                    max_te = max_te.max(tick + n.length);
                    max_k = max_k.max(key);
                    min_k = min_k.min(key);
                }
            }

            // 恢复 raw bounds 缓存，后续帧走 O(1) ghost 路径
            if any {
                self.selected_bounds
                    .set(Some((raw_min_t, raw_max_te, raw_max_k, raw_min_k)));
            }
        } else {
            puffin::profile_scope!("get_selection_box_bounds::fallback");
            // 兜底路径：selected_bounds 失效且非 ghost 时全量扫描
            for &i in selected.iter() {
                let Some(n) = data.get_note_view(i) else {
                    continue;
                };
                any = true;
                min_t = min_t.min(n.tick);
                max_te = max_te.max(n.tick + n.length);
                max_k = max_k.max(n.key);
                min_k = min_k.min(n.key);
            }
            // 恢复 selected_bounds 缓存
            if any {
                self.selected_bounds
                    .set(Some((min_t, max_te, max_k, min_k)));
            }
        }
        if !any {
            return None;
        }

        Some((
            view.tick_to_x(min_t),
            view.tick_to_x(max_te),
            view.key_to_y(max_k),
            view.key_to_y(min_k) + view.zoom_y,
        ))
    }

    pub fn hit_test_selection_box(&self, pos: Point) -> Option<SelectionHitType> {
        let bounds = self.get_selection_box_bounds()?;
        hit_test::hit_test_selection_box(bounds, (pos.x, pos.y))
    }
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
