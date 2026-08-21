//! 素材 / I2M 放置确认写入
//!
//! 2026-08-18 拆分：原 `root/handlers/material.rs`（803 行）按职责拆分，
//! 本模块承载放置生成的确认写入逻辑（逐轨写入 / 自动建轨 / CreateOp 历史）。

use crate::root::Root;
use crate::sidebar;

impl Root {
    /// 确认放置生成（I2M / 素材共用）：按逐轨写入/自动建轨策略写入 document
    ///
    /// - 轨 0 写入当前音轨；
    /// - 轨 1+ 优先复用现有非当前音轨，数量不足时才新建缺失数量的音轨
    ///   （sidebar + document 同步扩轨）；
    /// - 使用 `CreateOp` 操作日志记录（跨轨撤销/重做）。
    ///
    /// 素材放置复用此路径：素材音轨数 = preview.tracks.len()，
    /// Y 向偏移已由 `track_screen_notes` 应用（`note_screen_key`）。
    pub(crate) fn handle_i2m_placement_confirm(&mut self) {
        use lumino_editor_state::ImageToMidiMode;

        // 快照放置状态（避免与后续 &mut self 借用冲突）
        let i2m = self.editor.editor_state.image_to_midi.clone();
        if i2m.mode != ImageToMidiMode::Placing {
            return;
        }
        let Some(preview) = &i2m.preview else {
            return;
        };

        let current_track = self.editor.editor_state.data.current_track;
        // 收集每轨音符（区域映射后的屏幕 tick/key/length）
        let mut tracks_data: Vec<Vec<(f32, u8, f32)>> = Vec::with_capacity(preview.tracks.len());
        let mut total_notes = 0usize;
        for (idx, _) in preview.tracks.iter().enumerate() {
            let notes = i2m.track_screen_notes(idx);
            total_notes += notes.len();
            tracks_data.push(notes);
        }
        if total_notes == 0 {
            return;
        }

        // 音轨分配策略：轨 0 始终写入当前音轨；轨 1+ 优先复用现有非当前音轨
        // （按侧边栏顺序取用），数量不足时才新建缺失数量的音轨——避免多次
        // 放置都无脑新建 N-1 条轨道，导致音轨无限膨胀。
        let needed_extra = preview.tracks.len().saturating_sub(1);
        let reused_tracks: Vec<usize> = self
            .sidebar
            .tracks
            .iter()
            .map(|t| t.id)
            .filter(|id| *id != current_track)
            .take(needed_extra)
            .collect();
        let deficit = needed_extra.saturating_sub(reused_tracks.len());

        // 自动建轨：仅为不足的数量新建音轨（sidebar + document 同步）
        let before: std::collections::HashSet<usize> =
            self.sidebar.tracks.iter().map(|t| t.id).collect();
        for _ in 0..deficit {
            self.sidebar.update(sidebar::Event::AddTrack);
        }
        let new_track_ids: Vec<usize> = self
            .sidebar
            .tracks
            .iter()
            .filter(|t| !before.contains(&t.id))
            .map(|t| t.id)
            .collect();

        // 逐轨批量写入（O(N+M) 归并，单次重建，单轨单次 COW）
        // 旧路径逐音符 insert_note → N 次 COW 深拷 + N 条 delta → 数千音符卡死
        let mut create_ops: Vec<lumino_note_core::history::CreateOp> =
            Vec::with_capacity(total_notes);
        let mut affected = std::collections::HashSet::new();
        // 预分配：主轨增量脏标记需单独处理
        let mut main_track_needs_dirty = false;
        for (color_idx, notes) in tracks_data.iter().enumerate() {
            if notes.is_empty() {
                continue;
            }
            let target_track = if color_idx == 0 {
                current_track
            } else {
                let reuse_idx = color_idx - 1;
                reused_tracks
                    .get(reuse_idx)
                    .copied()
                    .or_else(|| new_track_ids.get(reuse_idx - reused_tracks.len()).copied())
                    .unwrap_or(current_track)
            };
            if !self.editor.editor_state.data.ensure_track(target_track) {
                continue;
            }
            // 批量归一化 + 一次性 Note→NoteEvent 转换（零逐条 insert）
            let mut track_events = Vec::with_capacity(notes.len());
            for &(tick, key, length) in notes {
                let tick = tick.round();
                let length = length.round().max(1.0);
                let note = lumino_note_core::note::Note::new(tick, u16::from(key), length);
                let event = lumino_editor_state::note_to_event(note);
                track_events.push(event);
            }
            // 历史：先基于 events 快照生成 CreateOp（Copy 开销 16B/条，可忽略）
            let create_ops_for_track: Vec<lumino_note_core::history::CreateOp> = track_events
                .iter()
                .map(|ev| lumino_note_core::history::CreateOp {
                    track_id: target_track as u32,
                    note: *ev,
                })
                .collect();
            // 关键优化：单次批量归并（内部自动排序），峰值仅单块 8MB，替代 N 次 COW
            let inserted = self
                .editor
                .editor_state
                .data
                .document
                .as_mut()
                .map(|doc| doc.batch_insert_notes(target_track, track_events))
                .unwrap_or(0);
            if inserted > 0 {
                create_ops.extend(create_ops_for_track);
                affected.insert(target_track);
                if target_track == current_track {
                    main_track_needs_dirty = true;
                }
                // 同步维护 max_end 缓存（batch 已处理）无需额外置脏
            }
        }

        // 历史记录 + 标记变化（主轨走全量 dirty，其余轨洋葱皮增量豁免）
        if !create_ops.is_empty() {
            self.editor
                .editor_state
                .data
                .history
                .push_note_create(create_ops);
            // 批量插入索引散布，增量 InsertAt 需 N 次 GPU 搬运 → 主轨直接全量
            if main_track_needs_dirty {
                self.editor.editor_state.data.note_delta_events.clear();
                self.editor.editor_state.data.note_delta_dirty = true;
            }
            self.editor
                .editor_state
                .data
                .mark_track_notes_changed_for(Some(affected.clone()));
            if main_track_needs_dirty {
                self.editor.editor_state.data.note_delta_dirty = true;
            }
        }

        // 清除放置模式，还原显示区域
        self.editor.editor_state.image_to_midi.cancel();
        self.right_sidebar.converting = false;
        // 完全还原工具：切回转换前的工具（√ 写入成功后流程结束）
        if let Some(tool) = self.i2m_restore_tool.take() {
            self.toolbar.current_tool = tool;
            self.editor.set_tool(tool);
        }
        // 清理放置前残留的交互状态：写入改变了音符索引，残留的选中集合与
        // pending_drag_state 仍指向写入前的索引，保留会导致后续调整音符长度时
        // 触发批量 ResizingSelection（连带周围音符长度改变）或 ghost 误偏移。
        self.editor.editor_state.interaction.selected_notes.clear();
        self.editor.clear_pending_drag();
        self.editor.mark_notes_changed();
        self.update_playback_notes();
        self.editor.clear_notes_changed();
        self.editor
            .invalidate_caches(lumino_ui_editor::CacheInvalidation::ALL);

        tracing::info!("放置写入完成：{} 个音符", total_notes);
    }
}
