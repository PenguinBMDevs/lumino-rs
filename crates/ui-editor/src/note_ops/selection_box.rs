//! 选择框边界计算：get_selection_box_rects + get_selection_box_bounds + hit_test_selection_box
//!
//! 三层路径优先级：
//! 1. 非 ghost 路径：O(1) 读取 `selected_bounds` 缓存
//! 2. ghost 路径：O(1) 缓存 + delta（拖拽中或松手后待提交）
//! 3. O(N) 回退路径：缓存失效兜底，同时恢复缓存供后续帧走 O(1)
//!
//! **复制模式双框**：复制（`pending_copy` / `DraggingSelectionCopy`）时返回
//! **两个独立框**——原件框（选中音符原位）与副本框（选中音符 + 复制 delta），
//! 渲染与命中测试各自独立；非复制模式返回单个框。

use iced_core::Point;

use super::super::{EditState, Editor, SelectionHitType};
use crate::rendering::{copy_deltas_for_index, is_copy_ghosted};
use lumino_editor_state::editor_state::hit_test;

/// 屏幕坐标矩形 (min_x, max_x, min_y, max_y)
type ScreenRect = (f32, f32, f32, f32);

impl Editor {
    /// 选择框矩形集合（屏幕坐标）
    ///
    /// - 复制模式（`pending_copy` / `DraggingSelectionCopy`）：返回两个独立框
    ///   ——原件框（仅移动 delta）与副本框（移动 + 复制 delta）
    /// - 其他状态：返回单个框（选中音符的包围盒）
    /// - 无选中：返回空 Vec
    ///
    /// 渲染与命中测试请使用本方法；`get_selection_box_bounds` 仅为兼容入口
    /// （返回所有框的并集）。
    pub fn get_selection_box_rects(&self) -> Vec<ScreenRect> {
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
            return Vec::new();
        }

        // 判断是否需要 ghost delta（拖拽中或松手后待提交状态）
        // 注意：DraggingSelection 也需要 ghost，否则 selection_box 不跟随拖动
        // 复制模式（DraggingSelectionCopy / pending_copy）同样需要：原件与副本
        // 各自拥有独立框选（否则副本没有"框选"视觉反馈）
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
            return vec![rect_from_bounds(view, min_t, max_te, max_k, min_k)];
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
            puffin::profile_scope!("get_selection_box_rects::ghost_o1");
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

            // 复制模式：**双独立框选**——原件框与副本框各自独立。
            // 原件框 = 选中音符 + 移动 delta；副本框 = 选中音符 + 移动 + 复制 delta。
            // 原件保持框选状态（修复 BUG 1），且从原件框再次 Ctrl+拖动可继续
            // 生成新副本（修复 BUG 2，不触发空白提交）。
            if copy_active {
                let mut rects = Vec::with_capacity(2);
                // 原件框（仅移动 delta）
                let (o_min_t, o_max_te, o_max_k, o_min_k) =
                    ghost_rect(min_t, max_te, max_k, min_k, move_dt, move_dk, max_key);
                rects.push(rect_from_bounds(view, o_min_t, o_max_te, o_max_k, o_min_k));
                // 副本框（移动 + 复制 delta）
                let (dt, dk) = (
                    move_dt.saturating_add(copy_dt),
                    move_dk.saturating_add(copy_dk),
                );
                let (g_min_t, g_max_te, g_max_k, g_min_k) =
                    ghost_rect(min_t, max_te, max_k, min_k, dt, dk, max_key);
                rects.push(rect_from_bounds(view, g_min_t, g_max_te, g_max_k, g_min_k));
                // 不缓存 ghost 结果（delta 每帧变化）
                return rects;
            }
            // 移动模式（无复制）：选择框跟随移动 ghost（原件 + move delta）
            if move_active {
                let (g_min_t, g_max_te, g_max_k, g_min_k) =
                    ghost_rect(min_t, max_te, max_k, min_k, move_dt, move_dk, max_key);
                // 不缓存 ghost 结果（delta 每帧变化）
                return vec![rect_from_bounds(view, g_min_t, g_max_te, g_max_k, g_min_k)];
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
            puffin::profile_scope!("get_selection_box_rects::ghost_on");
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

            // 复制模式：原件框与副本框各自独立收集边界
            let mut o_min_t = f32::INFINITY;
            let mut o_max_te = f32::NEG_INFINITY;
            let mut o_max_k = u16::MIN;
            let mut o_min_k = u16::MAX;
            let mut c_min_t = f32::INFINITY;
            let mut c_max_te = f32::NEG_INFINITY;
            let mut c_max_k = u16::MIN;
            let mut c_min_k = u16::MAX;

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
                    // 原件 ghost = note + (pending_drag + 移动 drag) delta
                    let mut m_dt = 0i64;
                    let mut m_dk = 0i16;
                    if let Some(pending) = pending
                        && i < pending.selected.len()
                        && pending.selected[i]
                    {
                        m_dt = pending.delta_tick;
                        m_dk = pending.delta_key;
                    }
                    match edit_state {
                        EditState::Dragging { drag_state, .. }
                        | EditState::DraggingSelection { drag_state }
                            if i < drag_state.selected.len() && drag_state.selected[i] =>
                        {
                            m_dt = m_dt.saturating_add(drag_state.delta_tick);
                            m_dk = m_dk.saturating_add(drag_state.delta_key);
                        }
                        _ => {}
                    }
                    let o_tick = (n.tick + m_dt as f32).max(0.0);
                    let o_key = (n.key as i32 + m_dk as i32).clamp(0, max_key as i32) as u16;
                    o_min_t = o_min_t.min(o_tick);
                    o_max_te = o_max_te.max(o_tick + n.length);
                    o_max_k = o_max_k.max(o_key);
                    o_min_k = o_min_k.min(o_key);
                    // 副本 ghost = note + (移动 + 复制) delta；
                    // 连续复制（pending_copy + DraggingSelectionCopy）时旧副本与新
                    // 副本并存，两条都计入副本框边界。
                    if is_copy_ghosted(i, pending_copy, edit_state) {
                        let (old_copy, new_copy) =
                            copy_deltas_for_index(i, pending_copy, pending, edit_state);
                        for (cdt, cdk) in [old_copy, new_copy].into_iter().flatten() {
                            let tick = (n.tick + cdt as f32).max(0.0);
                            let key = (n.key as i32 + cdk as i32).clamp(0, max_key as i32) as u16;
                            c_min_t = c_min_t.min(tick);
                            c_max_te = c_max_te.max(tick + n.length);
                            c_max_k = c_max_k.max(key);
                            c_min_k = c_min_k.min(key);
                        }
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
            if copy_active {
                // 双独立框：原件框 + 副本框
                let mut rects = Vec::with_capacity(2);
                if o_min_t.is_finite() {
                    rects.push(rect_from_bounds(view, o_min_t, o_max_te, o_max_k, o_min_k));
                }
                if c_min_t.is_finite() {
                    rects.push(rect_from_bounds(view, c_min_t, c_max_te, c_max_k, c_min_k));
                }
                return rects;
            }
            if !any {
                return Vec::new();
            }
            return vec![rect_from_bounds(view, min_t, max_te, max_k, min_k)];
        }

        puffin::profile_scope!("get_selection_box_rects::fallback");
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
        if !any {
            return Vec::new();
        }
        vec![rect_from_bounds(view, min_t, max_te, max_k, min_k)]
    }

    /// 兼容入口：返回所有选择框的并集（覆盖所有选中与副本）
    ///
    /// 语义与旧版 `get_selection_box_bounds` 一致；复制模式的独立双框请使用
    /// `get_selection_box_rects`。
    pub fn get_selection_box_bounds(&self) -> Option<ScreenRect> {
        let mut rects = self.get_selection_box_rects().into_iter();
        let first = rects.next()?;
        let (min_x, max_x, min_y, max_y) = rects
            .fold(first, |(a1, a2, a3, a4), (b1, b2, b3, b4)| {
                (a1.min(b1), a2.max(b2), a3.min(b3), a4.max(b4))
            });
        Some((min_x, max_x, min_y, max_y))
    }

    /// 命中检测：遍历所有选择框（复制模式含原件框 + 副本框），返回第一个命中
    pub fn hit_test_selection_box(&self, pos: Point) -> Option<SelectionHitType> {
        for bounds in self.get_selection_box_rects() {
            if let Some(hit) = hit_test::hit_test_selection_box(bounds, (pos.x, pos.y)) {
                return Some(hit);
            }
        }
        None
    }

    /// 检测位置是否命中**副本框**（第二个及以后的选择框）
    ///
    /// 复制模式双框：index 0 = 原件框，index ≥ 1 = 副本框。
    /// Ctrl+拖动副本框 = 从副本继续复制下一份；拖动原件框 = 移动原件。
    pub fn hit_test_copy_box(&self, pos: Point) -> bool {
        self.get_selection_box_rects()
            .into_iter()
            .skip(1)
            .any(|bounds| hit_test::hit_test_selection_box(bounds, (pos.x, pos.y)).is_some())
    }
}

/// (min_t, max_te, max_k, min_k) → 屏幕坐标矩形
#[inline]
fn rect_from_bounds(
    view: &lumino_core::view_state::ViewState,
    min_t: f32,
    max_te: f32,
    max_k: u16,
    min_k: u16,
) -> ScreenRect {
    (
        view.tick_to_x(min_t),
        view.tick_to_x(max_te),
        view.key_to_y(max_k),
        view.key_to_y(min_k) + view.zoom_y,
    )
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
