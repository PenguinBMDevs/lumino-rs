//! 拖动和调整大小操作

use super::{EditState, Editor};
use iced_core::Point;
use lumino_core::DragState;

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

    /// 根据当前编辑状态计算音符的变化量
    ///
    /// 返回 (new_tick, new_key, new_length, note_to_play)
    ///
    /// **ghost 方案**：`Dragging` / `DraggingSelection` 期间不写入 `data.notes`，
    /// 仅更新 `DragState` 的 delta 偏移。渲染层用 `ghost_position` 实时计算预览位置。
    /// `new_tick` / `new_key` 仅用于 `ResizingStart` / `ResizingEnd`（这些仍走直接写入路径）。
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
        let mut new_tick = None;
        let new_key = None;
        let mut new_length = None;
        let mut note_to_play = None;

        // 预读 Dragging 状态下音符原始位置（ghost 方案：drag 期间 data.notes 不变）
        let dragging_note_orig: Option<(f32, u16)> = match &self.editor_state.interaction.edit_state
        {
            EditState::Dragging { note_index, .. } => self
                .editor_state
                .data
                .notes
                .get(*note_index)
                .map(|n| (n.tick, n.key)),
            _ => None,
        };

        // 预读 ResizingSelection 状态下选中索引（兼容 selection_bitset 和 selected_notes）
        // 必须在 match 之前获取，避免 match 的 mutable borrow 与 self 方法冲突
        let resize_selected_indices: Vec<usize> = {
            let interaction = &self.editor_state.interaction;
            if let Some(ref bs) = interaction.selection_bitset {
                let mut indices = Vec::with_capacity(bs.count_ones());
                bs.for_each_set(|i| indices.push(i));
                indices
            } else {
                interaction.selected_notes.iter().copied().collect()
            }
        };

        match &mut self.editor_state.interaction.edit_state {
            EditState::Selecting { .. } => {
                self.update_selection();
                return (None, None, None, None);
            }
            EditState::Drawing { current_tick, .. } => {
                *current_tick = snapped_tick;
            }
            EditState::Dragging {
                note_index: _,
                drag_state,
                last_played_key,
            } => {
                let Some((original_tick, original_key)) = dragging_note_orig else {
                    return (None, None, None, None);
                };
                // drag_state.initial_tick 是 mouse 拖动开始时的 tick
                let raw_delta_tick = tick - drag_state.initial_tick as f32;
                let snapped_delta_tick = (raw_delta_tick / snap_precision).round() * snap_precision;
                // calculated_key = key - (mouse_initial_key - original_key)
                //                = key - mouse_initial_key + original_key
                let calculated_key = (key as i32 - drag_state.initial_key as i32
                    + original_key as i32)
                    .clamp(0, visible_key_count.saturating_sub(1) as i32)
                    as u16;

                // 更新 drag_state 的 delta（用于 ghost 渲染与松手时 apply_to_notes）
                // delta_tick = snapped_delta_tick（音符偏移量）
                // delta_key = calculated_key - original_key（音符 key 偏移量）
                let _ = original_tick; // original_tick 用于 ghost_position 内部 clamp，此处无需再用
                let delta_key = (calculated_key as i16).saturating_sub(original_key as i16);
                drag_state.set_delta(snapped_delta_tick as i64, delta_key);

                if calculated_key != *last_played_key {
                    note_to_play = Some(calculated_key);
                    *last_played_key = calculated_key;
                }
                // ghost 方案：dragging 期间不写 notes，不返回 new_tick/new_key
            }
            EditState::ResizingStart {
                original_tick,
                original_length,
                ..
            } => {
                let end_tick = *original_tick + *original_length;
                let calculated_tick = snapped_tick.min(end_tick - snap_precision).max(0.0);
                new_tick = Some(calculated_tick);
                new_length = Some(end_tick - calculated_tick);
            }
            EditState::ResizingEnd { note_index, .. } => {
                if let Some(note) = self.editor_state.data.notes.get(*note_index) {
                    new_length = Some((snapped_tick - note.tick).max(snap_precision));
                }
            }
            EditState::DraggingSelection { drag_state } => {
                crate::puffin_profiler::dragging_selection();
                // 变化量不在拖动过程中逐帧计算——所有选中音符的 delta 相同。
                // 这里只更新 drag_state.delta 供松开鼠标时一次性保存到 pending，
                // 不触发重绘（collect_visible_note_data 不走 ghost 路径）。
                // 视觉反馈：拖动期间音符显示原始位置，松开鼠标后 snap 到 ghost 位置。
                let raw_delta_tick = snapped_tick - drag_state.initial_tick as f32;
                let snapped_delta_tick = (raw_delta_tick / snap_precision).round() * snap_precision;
                let delta_tick_i = snapped_delta_tick as i64;
                let delta_key_i = (key as i32 - drag_state.initial_key as i32) as i16;

                if delta_tick_i != drag_state.delta_tick || delta_key_i != drag_state.delta_key {
                    drag_state.set_delta(delta_tick_i, delta_key_i);
                }
            }
            EditState::ResizingSelectionStart { last_tick } => {
                let delta_tick = snapped_tick - *last_tick;

                if delta_tick != 0.0 {
                    let selected = &resize_selected_indices;
                    let note_store_enabled = self.editor_state.data.note_store_enabled;

                    // 同时修改 data.notes（权威源）和 note_store（渲染读取源），
                    // 确保 note_store_enabled 时渲染路径能读到最新的音符数据。
                    if note_store_enabled {
                        for &i in selected {
                            self.editor_state.data.note_store.modify(i, |n| {
                                let new_length = n.length - delta_tick;
                                if new_length >= snap_precision {
                                    n.tick += delta_tick;
                                    n.length = new_length;
                                }
                            });
                        }
                    } else {
                        for &i in selected {
                            if let Some(note) = self.editor_state.data.notes.get_mut(i) {
                                let new_length = note.length - delta_tick;
                                if new_length >= snap_precision {
                                    note.tick += delta_tick;
                                    note.length = new_length;
                                }
                            }
                        }
                    }

                    *last_tick = snapped_tick;

                    // 增量更新 selected_bounds：所有音符右移 delta_tick → min_t 增加
                    // max_te 不变（tick+length 保持不变），min_k/max_k 不变
                    if let Some((min_t, max_te, max_k, min_k)) = self.selected_bounds.get() {
                        self.selected_bounds.set(Some((
                            (min_t + delta_tick).max(0.0),
                            max_te,
                            max_k,
                            min_k,
                        )));
                    }

                    // ghost 方案：Resizing 期间 notes 已改，但空间索引不每帧重建。
                    // 用 mark_ghost_dirty 只触发 wgpu 重绘（基于新 notes），不重建索引。
                    // 空间索引在松手时（released.rs）一次性 mark_notes_changed 重建。
                    // **性能关键**：1000W 音符建树 124ms，每帧重建 = 60fps × 124ms = 灾难。
                    self.mark_ghost_dirty();
                }
            }
            EditState::ResizingSelectionEnd { last_tick } => {
                let delta_tick = snapped_tick - *last_tick;

                if delta_tick != 0.0 {
                    let selected = &resize_selected_indices;
                    let note_store_enabled = self.editor_state.data.note_store_enabled;

                    if note_store_enabled {
                        for &i in selected {
                            self.editor_state.data.note_store.modify(i, |n| {
                                let new_length = n.length + delta_tick;
                                if new_length >= snap_precision {
                                    n.length = new_length;
                                }
                            });
                        }
                    } else {
                        for &i in selected {
                            if let Some(note) = self.editor_state.data.notes.get_mut(i) {
                                let new_length = note.length + delta_tick;
                                if new_length >= snap_precision {
                                    note.length = new_length;
                                }
                            }
                        }
                    }

                    *last_tick = snapped_tick;

                    // 增量更新 selected_bounds：所有音符长度增加 delta_tick → max_te 增加
                    // min_t 不变（tick 不变），min_k/max_k 不变
                    if let Some((min_t, max_te, max_k, min_k)) = self.selected_bounds.get() {
                        self.selected_bounds
                            .set(Some((min_t, max_te + delta_tick, max_k, min_k)));
                    }

                    // ghost 方案：同 ResizingSelectionStart，期间不重建索引
                    self.mark_ghost_dirty();
                }
            }
            _ => {}
        }

        (new_tick, new_key, new_length, note_to_play)
    }

    /// 更新框选区域中的音符选中状态
    ///
    /// 增量优化（V2）：
    /// - 缓存上一帧的 raw 边界，用 `rect_subtract` 计算新增/减少的薄条区域
    /// - 仅对 delta 区域执行 R-tree 查询，插入/删除边缘变化的部分音符
    /// - 避免每帧 O(N) 全量 HashSet 重建（60W 音符 = 76ms/帧 → 几乎归零）
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

        // ═══ 增量 delta 更新 ═══
        // 1. 无缓存 → 全量重建（首帧）
        // 2. 缓存相同 → 跳过（边界未变）
        // 3. 缓存不同 → 计算 delta 矩形，仅增删边缘变化部分
        let Some(old_bounds) = self.cached_selection_bounds.get() else {
            self.cached_selection_bounds.set(Some(new_bounds));
            puffin::profile_scope!("diag::selection_full_rebuild");
            self.rebuild_selected_notes(new_min_t, new_max_t, new_min_k, new_max_k);
            return;
        };

        if old_bounds == new_bounds {
            puffin::profile_scope!("diag::selection_cache_hit");
            // 安全保护：边界未变但选中被清空（如 selection_clear 在 start_new_selection 被调用），
            // 仍需重建，否则 selected_notes 保持空，二次框选时不会选中任何音符。
            if !self.has_selection() {
                self.cached_selection_bounds.set(Some(new_bounds));
                puffin::profile_scope!("diag::selection_full_rebuild");
                self.rebuild_selected_notes(new_min_t, new_max_t, new_min_k, new_max_k);
            }
            return;
        }

        // 安全保护：无选中音符但缓存在 → 某处被清了，做全量重建
        if !self.has_selection() {
            self.cached_selection_bounds.set(Some(new_bounds));
            puffin::profile_scope!("diag::selection_full_rebuild");
            self.rebuild_selected_notes(new_min_t, new_max_t, new_min_k, new_max_k);
            return;
        }

        self.cached_selection_bounds.set(Some(new_bounds));

        let (old_min_t, old_max_t, old_min_k, old_max_k) = old_bounds;

        // 确保空间索引就绪；若不存在则 fallback 全量重建
        self.ensure_spatial_index();
        if self.spatial.note_index.borrow().is_none() {
            puffin::profile_scope!("diag::selection_full_rebuild");
            self.rebuild_selected_notes(new_min_t, new_max_t, new_min_k, new_max_k);
            return;
        }

        // 注意：以下 borrow 块必须分开，避免 RefCell borrow 与 &mut self 的冲突
        let mut delta_rects: Vec<(f32, f32, u16, u16)> = Vec::with_capacity(8);

        // 计算减少区域（old 里有、new 里没有）
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

        let mut removed_indices = Vec::new();
        let mut added_indices = Vec::new();
        {
            let index = self.spatial.note_index.borrow();
            let index = index.as_ref().expect("just checked is_some");
            let mut cache = self.spatial.query_cache.borrow_mut();
            for &(t_min, t_max, k_min, k_max) in &delta_rects {
                cache.clear();
                index.update_query(t_min, t_max, k_min, k_max, &mut cache);
                removed_indices.extend(cache.iter().copied());
            }
        }
        delta_rects.clear();

        let removed = removed_indices
            .iter()
            .filter(|&&i| self.selection_remove(&i))
            .count();

        // 计算新增区域（new 里有、old 里没有）
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

        {
            let index = self.spatial.note_index.borrow();
            let index = index.as_ref().expect("just checked is_some");
            let mut cache = self.spatial.query_cache.borrow_mut();
            for &(t_min, t_max, k_min, k_max) in &delta_rects {
                cache.clear();
                index.update_query(t_min, t_max, k_min, k_max, &mut cache);
                added_indices.extend(cache.iter().copied());
            }
        }
        let added = added_indices.len();
        for i in added_indices {
            self.selection_insert(i);
        }

        puffin::profile_scope!("diag::selection_delta");

        // debug 日志：仅在 delta 较大时打印,避免高频刷屏
        if removed + added > 100 {
            tracing::debug!(
                "diag::selection_delta — 移除了 {} 个, 新增了 {} 个",
                removed,
                added,
            );
        }
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
