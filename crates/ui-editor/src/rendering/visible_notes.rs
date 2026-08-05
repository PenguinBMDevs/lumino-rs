//! 视口可见音符数据收集（渲染热路径）
//!
//! 拆分原因：rendering.rs 接近 400 行限制，按职责拆分。
//!
//! 性能优化：
//! - **NoteStore 启用时走零 clone 路径**：`get_ref` / `iter_refs` 返回 NoteView（Copy）
//!   16M 音符场景下避免每帧 K 个 Note 结构体 clone 开销（K=可见音符数）
//! - 渲染路径不触发 `ensure_spatial_index`，避免 133ms 全量重建
//! - dirty 时线性扫描，!dirty 且有索引时用索引查询
//! - 索引重建交给交互路径（hit_test_note / update_selection）按需触发

use super::Editor;
use super::ghost::{has_active_ghost_delta, is_note_ghosted};
use crate::EditState;

impl Editor {
    /// 收集当前视口内可见的音符数据（tick, key, length）
    ///
    /// `overscan_factor` 用于扩展查询范围，减少频繁重建。0.0 表示精确视口。
    /// 返回可见音符数量，结果写入传入的 buffer。
    ///
    /// `indices`（可选）：并行收集可见音符的 notes 索引（升序输出前需调用方
    /// 排序——索引路径按空间索引查询顺序，线性路径按遍历顺序，均天然升序）。
    /// 供主音轨事件级增量映射（notes 索引 → GPU 位置）使用。
    ///
    /// **ghost 方案**：返回的数据已应用 `pending_drag_state` 与当前 `drag_state`
    /// 的偏移，确保拖动期间主音轨音符（蓝色）的渲染位置与视觉反馈一致。
    ///
    /// 性能优化：
    /// - 渲染路径**不触发** `ensure_spatial_index`，避免移动提交后 133ms 全量重建
    /// - `dirty` 时走线性扫描（O(N)，N=50000 仅 ~0.5ms），`!dirty` 且有索引时用索引查询
    /// - 索引重建交给交互路径（`hit_test_note` / `update_selection`）按需触发
    /// - **NoteStore 启用时走零 clone 路径**：`get_ref` / `iter_refs` 返回 NoteView（Copy）
    pub fn collect_visible_note_data(
        &self,
        result: &mut Vec<(f32, u16, f32)>,
        mut indices: Option<&mut Vec<usize>>,
        overscan_factor: f32,
    ) -> usize {
        crate::puffin_profiler::collect_visible_note_data();
        result.clear();
        if let Some(idx) = indices.as_deref_mut() {
            idx.clear();
        }

        let (visible_tick_start, visible_tick_end, visible_key_min, visible_key_max) =
            self.compute_visible_range(overscan_factor);

        let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);
        let edit_state = &self.editor_state.interaction.edit_state;
        let pending = &self.pending_drag_state;

        // 渲染路径：仅在索引干净（!dirty 且已存在）时复用，否则线性扫描。
        // 避免渲染帧触发 133ms 的全量重建——重建交给交互路径按需完成。
        let has_clean_index =
            !self.spatial.note_index_dirty.get() && self.spatial.note_index.borrow().is_some();

        // 性能优化：当没有活跃拖动时，ghost_delta_for_index 对每个音符都返回 None，
        // 避免 200 万次函数调用开销。先判断再决定走哪条路径。
        let needs_ghost = has_active_ghost_delta(pending, edit_state);

        if has_clean_index {
            self.collect_via_index(
                result,
                indices,
                visible_tick_start,
                visible_tick_end,
                visible_key_min,
                visible_key_max,
                max_key,
                edit_state,
                pending,
                needs_ghost,
            );
        } else {
            self.collect_via_linear_scan(
                result,
                indices,
                visible_tick_start,
                visible_tick_end,
                visible_key_min,
                visible_key_max,
                max_key,
                edit_state,
                pending,
                needs_ghost,
            );
        }

        result.len()
    }
}

impl Editor {
    /// 索引路径：空间索引 query 出 indices，再逐个取音符
    ///
    /// 参数较多是因为 hot path 避免参数打包结构体的 Clone/copy 开销，
    /// 全部为只读引用或 Copy 类型，直接传参零成本。
    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn collect_via_index(
        &self,
        result: &mut Vec<(f32, u16, f32)>,
        mut indices: Option<&mut Vec<usize>>,
        visible_tick_start: f32,
        visible_tick_end: f32,
        visible_key_min: u16,
        visible_key_max: u16,
        max_key: u16,
        edit_state: &EditState,
        pending: &Option<lumino_editor_state::DragState>,
        needs_ghost: bool,
    ) {
        let index = self.spatial.note_index.borrow();
        let index = match index.as_ref() {
            Some(idx) => idx,
            None => return,
        };
        let mut indices_buf = Vec::new();
        index.update_query(
            visible_tick_start,
            visible_tick_end,
            visible_key_min,
            visible_key_max,
            &mut indices_buf,
        );

        let (drag_dt, drag_dk) = current_drag_delta(edit_state);

        let data = &self.editor_state.data;
        // 2026-08 单一权威源：current_track_notes 返回 &[NoteEvent]（u32 tick/u8 key）
        let track_notes = data.current_track_notes();

        if needs_ghost {
            // 只有 pending 或 Dragging（单音符）会进入此分支。
            // DraggingSelection 不走此路径——变化量只在松开鼠标时计算一次。
            for &i in &indices_buf {
                if let Some(idx_out) = indices.as_deref_mut() {
                    idx_out.push(i);
                }
                if let Some(note) = track_notes.get(i) {
                    let (tick, key) = if is_note_ghosted(i, pending, edit_state) {
                        apply_ghost_delta(
                            note.start_tick as f32,
                            note.key as u16,
                            drag_dt,
                            drag_dk,
                            pending,
                            i,
                            max_key,
                        )
                    } else {
                        (note.start_tick as f32, note.key as u16)
                    };
                    result.push((tick, key, (note.end_tick - note.start_tick) as f32));
                }
            }
        } else {
            for &i in &indices_buf {
                if let Some(idx_out) = indices.as_deref_mut() {
                    idx_out.push(i);
                }
                if let Some(note) = track_notes.get(i) {
                    result.push((
                        note.start_tick as f32,
                        note.key as u16,
                        (note.end_tick - note.start_tick) as f32,
                    ));
                }
            }
        }
    }

    /// 线性扫描路径：索引脏或不存在时使用
    ///
    /// 参数较多是因为 hot path 避免参数打包结构体的 Clone/copy 开销，
    /// 全部为只读引用或 Copy 类型，直接传参零成本。
    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn collect_via_linear_scan(
        &self,
        result: &mut Vec<(f32, u16, f32)>,
        mut indices: Option<&mut Vec<usize>>,
        visible_tick_start: f32,
        visible_tick_end: f32,
        visible_key_min: u16,
        visible_key_max: u16,
        max_key: u16,
        edit_state: &EditState,
        pending: &Option<lumino_editor_state::DragState>,
        needs_ghost: bool,
    ) {
        let (drag_dt, drag_dk) = current_drag_delta(edit_state);
        let data = &self.editor_state.data;
        // 2026-08 单一权威源：current_track_notes 返回 &[NoteEvent]（u32 tick/u8 key）
        let track_notes = data.current_track_notes();

        if needs_ghost {
            for (i, note) in track_notes.iter().enumerate() {
                let (tick, key) = if is_note_ghosted(i, pending, edit_state) {
                    apply_ghost_delta(
                        note.start_tick as f32,
                        note.key as u16,
                        drag_dt,
                        drag_dk,
                        pending,
                        i,
                        max_key,
                    )
                } else {
                    (note.start_tick as f32, note.key as u16)
                };
                let note_end = note.end_tick as f32;
                if key >= visible_key_min
                    && key <= visible_key_max
                    && note_end >= visible_tick_start
                    && tick <= visible_tick_end
                {
                    if let Some(idx_out) = indices.as_deref_mut() {
                        idx_out.push(i);
                    }
                    result.push((tick, key, (note.end_tick - note.start_tick) as f32));
                }
            }
        } else {
            for (i, note) in track_notes.iter().enumerate() {
                let note_end = note.end_tick as f32;
                if note.key as u16 >= visible_key_min
                    && note.key as u16 <= visible_key_max
                    && note_end >= visible_tick_start
                    && note.start_tick as f32 <= visible_tick_end
                {
                    if let Some(idx_out) = indices.as_deref_mut() {
                        idx_out.push(i);
                    }
                    result.push((
                        note.start_tick as f32,
                        note.key as u16,
                        (note.end_tick - note.start_tick) as f32,
                    ));
                }
            }
        }
    }
}

/// 从 EditState 提取当前 drag delta（Dragging 和 DraggingSelection 均有非零值）
///
/// `DraggingSelection` 也纳入提取，确保第二次拖动（已有 pending 时）
/// 当前 drag_state 的 delta 能应用到渲染位置，避免音符视觉位置不随鼠标移动。
#[inline]
fn current_drag_delta(edit_state: &EditState) -> (i64, i16) {
    match edit_state {
        EditState::Dragging { drag_state, .. } | EditState::DraggingSelection { drag_state } => {
            (drag_state.delta_tick, drag_state.delta_key)
        }
        _ => (0i64, 0i16),
    }
}

impl Editor {
    /// 是否存在活跃的 ghost 拖动（拖动中 / pending 异步提交）
    ///
    /// UI 层（渲染线程）判断是否走 ghost 增量路径。
    pub fn has_active_ghost_delta_state(&self) -> bool {
        has_active_ghost_delta(
            &self.pending_drag_state,
            &self.editor_state.interaction.edit_state,
        )
    }

    /// 构建 ghost 拖动增量的可见位置数据（仅「选中 ∩ 可见」的索引）
    ///
    /// 卷帘拖动增量（2026-08-05）：拖动期间 document 未变（ghost 方案），
    /// 只有被拖动的音符需要更新渲染位置。本方法返回
    /// `(可见列表位置, ghost 后位置 (tick, key, length))`（按可见位置升序），
    /// UI 层据此生成 UpdateMany 增量发送——替代每帧全量 collect+build+上传。
    ///
    /// 调用前提：`has_active_ghost_delta` 为 true 且 `visible_indices` 与
    /// 上次全量构建的可见索引一致（GPU 位置 = 列表下标）。
    pub fn build_ghost_delta_positions(
        &self,
        visible_indices: &[usize],
    ) -> Vec<(usize, (f32, u16, f32))> {
        let edit_state = &self.editor_state.interaction.edit_state;
        let pending = &self.pending_drag_state;
        let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);
        let data = &self.editor_state.data;
        let (drag_dt, drag_dk) = current_drag_delta(edit_state);

        let mut out = Vec::new();
        for (pos, &note_idx) in visible_indices.iter().enumerate() {
            if !is_note_ghosted(note_idx, pending, edit_state) {
                continue;
            }
            let Some((tick, key, length)) = data
                .get_note_view(note_idx)
                .map(|n| (n.tick, n.key, n.length))
            else {
                continue;
            };
            let (tick, key) =
                apply_ghost_delta(tick, key, drag_dt, drag_dk, pending, note_idx, max_key);
            out.push((pos, (tick, key, length)));
        }
        out
    }
}

/// 应用 ghost delta 到音符位置
///
/// 合并 drag_state delta 和 pending delta（如果音符在 pending 选中集合中）。
#[inline]
fn apply_ghost_delta(
    tick: f32,
    key: u16,
    drag_dt: i64,
    drag_dk: i16,
    pending: &Option<lumino_editor_state::DragState>,
    i: usize,
    max_key: u16,
) -> (f32, u16) {
    let mut dt = drag_dt;
    let mut dk = drag_dk;
    if let Some(pending) = pending
        && i < pending.selected.len()
        && pending.selected[i]
    {
        dt = dt.saturating_add(pending.delta_tick);
        dk = dk.saturating_add(pending.delta_key);
    }
    (
        (tick + dt as f32).max(0.0),
        (key as i32 + dk as i32).clamp(0, max_key as i32) as u16,
    )
}
