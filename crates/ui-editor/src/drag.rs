//! 拖动和调整大小操作

mod compute_state_changes;

use super::{EditState, Editor};
use compute_state_changes::*;
use iced_core::Point;
use lumino_editor_state::DragState;

impl Editor {
    /// 检查是否应从 PendingDrag 转换到 Dragging 状态
    pub(crate) fn try_transition_to_dragging(&mut self, pos: iced_core::Point) {
        crate::puffin_profiler::try_transition_to_dragging();
        let EditState::PendingDrag {
            note_index,
            start_pos,
            original_tick,
            original_key,
        } = self.editor_state.interaction.edit_state
        else {
            return;
        };

        if !self.should_start_dragging(pos, Point::new(start_pos.0, start_pos.1)) {
            return;
        }

        // ghost 方案：拖动期间数据不动，仅维护 DragState 偏移
        let note_count = self.editor_state.data.notes.len();
        let drag_state = DragState::from_single(
            note_index,
            note_count,
            original_tick as i64,
            original_key as i16,
        );
        // 更新 editor_state
        self.editor_state.interaction.edit_state = EditState::Dragging {
            note_index,
            drag_state,
            last_played_key: original_key,
        };
    }

    /// 根据编辑状态计算音符变化量 → (new_tick, new_key, new_length, note_to_play)
    pub(crate) fn compute_state_changes(
        &mut self,
        tick: f32,
        key: u16,
        snapped_tick: f32,
    ) -> (Option<f32>, Option<u16>, Option<f32>, Option<u16>) {
        crate::puffin_profiler::compute_state_changes();
        let v = &self.editor_state.view;
        let snap_precision = v.snap_precision;
        let visible_key_count = v.visible_key_count;
        if matches!(
            self.editor_state.interaction.edit_state,
            EditState::Selecting { .. }
        ) {
            self.update_selection();
            return (None, None, None, None);
        }
        let sel: Vec<usize> = {
            let i = &self.editor_state.interaction;
            if let Some(ref bs) = i.selection_bitset {
                let mut v = Vec::with_capacity(bs.count_ones());
                bs.for_each_set(|i| v.push(i));
                v
            } else {
                i.selected_notes.iter().copied().collect()
            }
        };
        let (mut new_tick, mut new_length, mut note_to_play) = (None, None, None);
        match &mut self.editor_state.interaction.edit_state {
            EditState::Drawing { current_tick, .. } => handle_drawing(current_tick, snapped_tick),
            EditState::Dragging {
                note_index,
                drag_state,
                last_played_key,
            } => {
                let orig = self
                    .editor_state
                    .data
                    .notes
                    .get(*note_index)
                    .map(|n| (n.tick, n.key));
                note_to_play = handle_dragging(
                    drag_state,
                    last_played_key,
                    tick,
                    key,
                    snapped_tick,
                    snap_precision,
                    visible_key_count,
                    &orig,
                );
            }
            EditState::ResizingStart {
                original_tick,
                original_length,
                ..
            } => {
                (new_tick, new_length) = handle_resizing_start(
                    *original_tick,
                    *original_length,
                    snapped_tick,
                    snap_precision,
                );
            }
            EditState::ResizingEnd { note_index, .. } => {
                new_length = handle_resizing_end(
                    &self.editor_state.data.notes,
                    *note_index,
                    snapped_tick,
                    snap_precision,
                );
            }
            EditState::DraggingSelection { drag_state } => {
                handle_dragging_selection(drag_state, key, snapped_tick, snap_precision);
            }
            EditState::ResizingSelectionStart { last_tick } => {
                if handle_resizing_selection_start(
                    last_tick,
                    snapped_tick,
                    snap_precision,
                    &sel,
                    &mut self.editor_state.data.note_store,
                    &mut self.editor_state.data.notes,
                    &self.selected_bounds,
                    self.editor_state.data.note_store_enabled,
                ) {
                    self.mark_ghost_dirty();
                }
            }
            EditState::ResizingSelectionEnd { last_tick } => {
                if handle_resizing_selection_end(
                    last_tick,
                    snapped_tick,
                    snap_precision,
                    &sel,
                    &mut self.editor_state.data.note_store,
                    &mut self.editor_state.data.notes,
                    &self.selected_bounds,
                    self.editor_state.data.note_store_enabled,
                ) {
                    self.mark_ghost_dirty();
                }
            }
            _ => {}
        }
        (new_tick, None, new_length, note_to_play)
    }

    /// 增量更新框选：缓存旧边界 → rect_subtract → 仅查 delta 区域（非 O(N) 全量）
    pub(crate) fn update_selection(&mut self) {
        crate::puffin_profiler::update_selection();
        puffin::profile_scope!("diag::update_selection_total");

        // 非 Selecting 状态 → 清除缓存并返回
        if !matches!(
            self.editor_state.interaction.edit_state,
            EditState::Selecting { .. }
        ) {
            self.cached_selection_bounds.set(None);
            return;
        }

        let (start_tick, start_key, current_tick, current_key) =
            match &self.editor_state.interaction.edit_state {
                EditState::Selecting {
                    start_tick,
                    start_key,
                    current_tick,
                    current_key,
                    ..
                } => (*start_tick, *start_key, *current_tick, *current_key),
                _ => unreachable!("confirmed Selecting above"),
            };

        let new_min_t = start_tick.min(current_tick);
        let new_max_t = start_tick.max(current_tick);
        let new_min_k = start_key.min(current_key);
        let new_max_k = start_key.max(current_key);
        let new_bounds = (new_min_t, new_max_t, new_min_k, new_max_k);

        // 无缓存 → 全量重建（首帧）
        let Some(old_bounds) = self.cached_selection_bounds.get() else {
            self.cached_selection_bounds.set(Some(new_bounds));
            return rebuild_full_selection(self, new_min_t, new_max_t, new_min_k, new_max_k);
        };

        // 缓存命中 → 跳过或保护性重建
        if old_bounds == new_bounds {
            puffin::profile_scope!("diag::selection_cache_hit");
            if !self.has_selection() {
                self.cached_selection_bounds.set(Some(new_bounds));
                rebuild_full_selection(self, new_min_t, new_max_t, new_min_k, new_max_k);
            }
            return;
        }

        // 安全保护：无选中音符但缓存在 → 全量重建
        if !self.has_selection() {
            self.cached_selection_bounds.set(Some(new_bounds));
            return rebuild_full_selection(self, new_min_t, new_max_t, new_min_k, new_max_k);
        }

        self.cached_selection_bounds.set(Some(new_bounds));
        apply_selection_delta(self, old_bounds, new_min_t, new_max_t, new_min_k, new_max_k);
    }

    /// 全量重建 selected_notes（首帧 / fallback）
    fn rebuild_selected_notes(&mut self, min_tick: f32, max_tick: f32, min_key: u16, max_key: u16) {
        self.selection_clear();

        self.ensure_spatial_index();
        let indices: Vec<usize> = if let Some(index) = self.spatial.note_index.borrow().as_ref() {
            let mut cache = self.spatial.query_cache.borrow_mut();
            cache.clear();
            index.update_query(min_tick, max_tick, min_key, max_key, &mut cache);
            cache.iter().copied().collect()
        } else {
            puffin::profile_scope!("diag::selection_linear_scan");
            let note_count = self.editor_state.data.notes.len();
            tracing::debug!(
                "diag::selection_linear_scan — 音符数={}（无空间索引，线性回退）",
                note_count
            );
            self.editor_state
                .data
                .notes
                .iter()
                .enumerate()
                .filter(|&(_, note)| {
                    let note_end = note.tick + note.length;
                    note_end >= min_tick
                        && note.tick <= max_tick
                        && note.key >= min_key
                        && note.key <= max_key
                })
                .map(|(i, _)| i)
                .collect()
        };
        for i in indices {
            self.selection_insert(i);
        }
    }
}

/// 全量重建封装（用于从 `update_selection` 的 guard 块中调用）
fn rebuild_full_selection(
    editor: &mut Editor,
    min_tick: f32,
    max_tick: f32,
    min_key: u16,
    max_key: u16,
) {
    puffin::profile_scope!("diag::selection_full_rebuild");
    editor.rebuild_selected_notes(min_tick, max_tick, min_key, max_key);
}

/// 计算增量 delta 并应用移除/新增的区域
fn apply_selection_delta(
    editor: &mut Editor,
    old_bounds: (f32, f32, u16, u16),
    new_min_t: f32,
    new_max_t: f32,
    new_min_k: u16,
    new_max_k: u16,
) {
    let (old_min_t, old_max_t, old_min_k, old_max_k) = old_bounds;
    editor.ensure_spatial_index();
    if editor.spatial.note_index.borrow().is_none() {
        return rebuild_full_selection(editor, new_min_t, new_max_t, new_min_k, new_max_k);
    }

    let mut delta_rects: Vec<(f32, f32, u16, u16)> = Vec::with_capacity(8);

    // 减少区域（old 里有，new 里没有）
    rect_subtract(
        old_min_t,
        old_max_t,
        old_min_k,
        old_max_k,
        new_min_t,
        new_max_t,
        new_min_k,
        new_max_k,
        &mut delta_rects,
    );

    let remove_list = {
        let index = editor.spatial.note_index.borrow();
        let index = index.as_ref().expect("just checked is_some");
        let mut cache = editor.spatial.query_cache.borrow_mut();
        let mut list = Vec::new();
        for &(t_min, t_max, k_min, k_max) in &delta_rects {
            cache.clear();
            index.update_query(t_min, t_max, k_min, k_max, &mut cache);
            list.extend(cache.iter().copied());
        }
        list
    };
    let removed = remove_list.len();
    for i in remove_list {
        editor.selection_remove(&i);
    }
    delta_rects.clear();

    // 新增区域（new 里有，old 里没有）
    rect_subtract(
        new_min_t,
        new_max_t,
        new_min_k,
        new_max_k,
        old_min_t,
        old_max_t,
        old_min_k,
        old_max_k,
        &mut delta_rects,
    );

    let added_indices = {
        let index = editor.spatial.note_index.borrow();
        let index = index.as_ref().expect("just checked is_some");
        let mut cache = editor.spatial.query_cache.borrow_mut();
        let mut list = Vec::new();
        for &(t_min, t_max, k_min, k_max) in &delta_rects {
            cache.clear();
            index.update_query(t_min, t_max, k_min, k_max, &mut cache);
            list.extend(cache.iter().copied());
        }
        list
    };
    let added = added_indices.len();
    for i in added_indices {
        editor.selection_insert(i);
    }

    puffin::profile_scope!("diag::selection_delta");
    if removed + added > 100 {
        tracing::debug!(
            "diag::selection_delta — 移除了 {} 个, 新增了 {} 个",
            removed,
            added
        );
    }
}

/// 矩形差集：outer - inner = outer 中不在 inner 内的部分。
/// 返回最多 4 个非重叠矩形的列表。
///
/// 算法：先 clamp inner 到 outer 边界，然后从上/下/左/右四个方向切 strip。
/// 上/下 strip 跨越 outer 全宽，左/右 strip 夹在 inner 的垂直范围内 → 不重复。
#[allow(clippy::too_many_arguments)]
fn rect_subtract(
    outer_t_min: f32,
    outer_t_max: f32,
    outer_k_min: u16,
    outer_k_max: u16,
    inner_t_min: f32,
    inner_t_max: f32,
    inner_k_min: u16,
    inner_k_max: u16,
    result: &mut Vec<(f32, f32, u16, u16)>,
) {
    // Clamp inner to outer bounds
    let ic_t_min = inner_t_min.max(outer_t_min);
    let ic_t_max = inner_t_max.min(outer_t_max);
    let ic_k_min = inner_k_min.max(outer_k_min);
    let ic_k_max = inner_k_max.min(outer_k_max);

    // 无重叠 → 整个 outer 都是差集
    if ic_t_min >= ic_t_max || ic_k_min >= ic_k_max {
        result.push((outer_t_min, outer_t_max, outer_k_min, outer_k_max));
        return;
    }

    // 上 strip（outer 在 ic 上方的部分，对应更小的 key 值）
    if ic_k_min > outer_k_min {
        result.push((outer_t_min, outer_t_max, outer_k_min, ic_k_min));
    }
    // 下 strip（outer 在 ic 下方的部分，对应更大的 key 值）
    if ic_k_max < outer_k_max {
        result.push((outer_t_min, outer_t_max, ic_k_max, outer_k_max));
    }
    // 左 strip（outer 在 ic 左侧、上下之间）
    if ic_t_min > outer_t_min {
        result.push((outer_t_min, ic_t_min, ic_k_min, ic_k_max));
    }
    // 右 strip（outer 在 ic 右侧、上下之间）
    if ic_t_max < outer_t_max {
        result.push((ic_t_max, outer_t_max, ic_k_min, ic_k_max));
    }
}

impl Editor {
    fn should_start_dragging(&self, pos: iced_core::Point, start_pos: iced_core::Point) -> bool {
        let delta_x = pos.x - start_pos.x;
        let delta_y = pos.y - start_pos.y;
        let key_threshold = self.editor_state.view.zoom_y * DRAG_START_THRESHOLD_RATIO;
        let distance = (delta_x * delta_x + delta_y * delta_y).sqrt();
        let started = distance > key_threshold;
        if started {
            tracing::info!(
                "Editor: 拖动启动 - delta=({}, {}), distance={}, threshold={}",
                delta_x,
                delta_y,
                distance,
                key_threshold
            );
        }
        started
    }

    /// 完成单音符拖动（ghost 方案）
    ///
    /// 松手时一次性将 `drag_state.delta` 应用到 `data.notes`，并发送 `LocalNoteMoved` 协作同步事件。
    /// 返回 `true` 表示音符位置确实发生了变化。
    pub(crate) fn finalize_dragging(&mut self, note_index: usize, drag_state: DragState) -> bool {
        crate::puffin_profiler::finalize_dragging();
        if drag_state.is_delta_zero() {
            tracing::debug!("Editor: 单音符拖动 delta 为零，跳过提交");
            return false;
        }

        // 读取原始位置（apply 前的状态，用于协作同步事件）
        let (original_tick, original_key, length, current_track) = {
            let notes = &self.editor_state.data.notes;
            let Some(original_note) = notes.get(note_index) else {
                return false;
            };
            (
                original_note.tick,
                original_note.key,
                original_note.length,
                self.editor_state.data.current_track,
            )
        };

        let tick_offset = drag_state.delta_tick as f32;
        let key_offset = drag_state.delta_key;
        let max_key = self.editor_state.view.visible_key_count.saturating_sub(1);

        // NoteMove 操作日志化：先捕获 MoveOp（记录 apply 前的原始位置），再应用数据
        let ops = self.editor_state.data.move_ops_from_drag_state(&drag_state);

        // ghost 方案：流式应用 delta 到 notes 与当前 track_notes 缓存
        let modified = self
            .editor_state
            .data
            .apply_drag_state_streaming(&drag_state, max_key);
        if modified == 0 {
            tracing::debug!("Editor: 单音符拖动未产生实际变更（snap 后 delta 为零）");
            return false;
        }

        if !ops.is_empty() {
            self.editor_state.data.push_move_op(ops);
        }

        tracing::info!(
            "Editor: 音符移动完成 - original=({}, {}), offset=({}, {})",
            original_tick,
            original_key,
            tick_offset,
            key_offset
        );
        lumino_event::emit(lumino_event::Event::Window(
            lumino_event::window::Event::local_note_moved(
                original_tick,
                original_key,
                length,
                tick_offset,
                key_offset,
                current_track,
            ),
        ));
        true
    }

    // 注：原 `finalize_selection_dragging` 已移除——延迟提交方案下，松手保存到
    // `pending_drag_state`，真正提交在 `commit_pending_drag`（点击空白处或
    // `commit_current_edit` 时触发）。详见 `interaction/released.rs`。
}

use lumino_ui_constants::editor::DRAG_START_THRESHOLD_RATIO;
