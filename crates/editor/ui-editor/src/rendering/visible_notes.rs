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
use super::ghost::{has_active_ghost_delta, is_note_ghosted, push_copy_instances};
use super::ghost_positions::{apply_ghost_delta, current_drag_delta};
use crate::EditState;

/// 视口窗口查询的 lookback 上界（tick）
///
/// `ChunkedList` 按 `start_tick` 排序，视口查询需向前回溯以覆盖「跨入」
/// 长音符（起点在视口左侧、长度延伸进视口）。取 1M tick（≈ 2600 小节 @
/// 1920ppq 或 34 分钟 @ 120bpm 480ppq）——工程上音符长度不超过此值，
/// 超出则视口外左侧部分本就不渲染，影响可忽略。
const NOTES_WINDOW_LOOKBACK: u32 = 1_000_000;

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
    /// **复制模式**（`DraggingSelectionCopy` / `pending_copy_drag_state`）：
    /// 原始音符在原位置渲染，副本在偏移位置额外追加一条实例。
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
        let pending_copy = &self.pending_copy_drag_state;

        // 渲染路径：仅在索引干净（!dirty 且已存在）时复用，否则线性扫描。
        // 避免渲染帧触发 133ms 的全量重建——重建交给交互路径按需完成。
        let has_clean_index =
            !self.spatial.note_index_dirty.get() && self.spatial.note_index.borrow().is_some();

        // 性能优化：当没有活跃拖动时，ghost_delta_for_index 对每个音符都返回 None，
        // 避免 200 万次函数调用开销。先判断再决定走哪条路径。
        let needs_ghost = has_active_ghost_delta(pending, pending_copy, edit_state);

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
                pending_copy,
                needs_ghost,
            );
        } else {
            // 索引脏 / 未建（1600W 超大型工程不建索引）→ 窗口扫描
            self.collect_via_window(
                result,
                indices,
                visible_tick_start,
                visible_tick_end,
                visible_key_min,
                visible_key_max,
                max_key,
                edit_state,
                pending,
                pending_copy,
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
        pending_copy: &Option<lumino_editor_state::DragState>,
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
                    let length = (note.end_tick - note.start_tick) as f32;
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
                    result.push((tick, key, length));
                    // 复制模式：原始位置保留，副本在偏移位置追加实例。
                    // 连续复制（pending_copy + DraggingSelectionCopy）时旧副本
                    // 保持原位、新副本跟手——两条副本并存（"复制下一份"）。
                    push_copy_instances(
                        result,
                        note.start_tick as f32,
                        note.key as u16,
                        length,
                        max_key,
                        i,
                        pending_copy,
                        pending,
                        edit_state,
                        |_, _| true,
                    );
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

    /// 窗口扫描路径：空间索引不可用（脏 / 超大型不建索引）时使用
    ///
    /// 普通线性扫描 O(N)——1600W 音符工程插入后每帧全扫 ~160ms，正是「编辑
    /// 中间插入 4s + 内存 2-3G」的渲染侧大本营。本方法经 `ChunkedList::window_range`
    /// 块级二分框出视口 tick 窗口（向左 lookback 回溯「跨入」长音符），只遍历
    /// 窗口内音符，复杂度 O(log 块数 + 窗口长度) 与总音符量无关。
    ///
    /// 正确性：窗口下界 = 视口起点 - `NOTES_WINDOW_LOOKBACK`。音符起点早于
    /// 下界且长度超过 lookback 的极端超长音符会漏出（工程上音符长度
    /// < 1M tick ≈ 2600 小节 @ 1920ppq，可忽略）。原语义完整保留：过滤条件
    /// 仍签名结束判定（`end >= visible_tick_start`），窗口仅做剪枝。
    #[allow(clippy::too_many_arguments)]
    #[inline]
    fn collect_via_window(
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
        pending_copy: &Option<lumino_editor_state::DragState>,
        needs_ghost: bool,
    ) {
        let (drag_dt, drag_dk) = current_drag_delta(edit_state);
        let data = &self.editor_state.data;
        // 2026-08 单一权威源：current_track_notes 返回分块容器（u32 tick/u8 key）
        let track_notes = data.current_track_notes();
        let start_tick = visible_tick_start.max(0.0) as u32;
        let end_tick = visible_tick_end.max(0.0) as u32;
        if end_tick < start_tick {
            return;
        }
        let (lo, hi) = track_notes.window_range(start_tick, end_tick + 1, NOTES_WINDOW_LOOKBACK);

        if needs_ghost {
            for (i, note) in track_notes.iter_window(lo, hi) {
                let length = (note.end_tick - note.start_tick) as f32;
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
                    result.push((tick, key, length));
                    // 复制模式：原始位置保留，副本在偏移位置追加实例（同 collect_via_index）
                    push_copy_instances(
                        result,
                        note.start_tick as f32,
                        note.key as u16,
                        length,
                        max_key,
                        i,
                        pending_copy,
                        pending,
                        edit_state,
                        |copy_tick, copy_key| {
                            copy_key >= visible_key_min
                                && copy_key <= visible_key_max
                                && copy_tick <= visible_tick_end
                        },
                    );
                }
            }
        } else {
            for (i, note) in track_notes.iter_window(lo, hi) {
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
