//! 编辑器操作 - 音轨管理
//!
//! 2026-08 单一权威源改造：音符数据唯一权威是 `EditorData.document`，
//! 加载/删除/恢复音轨直接整轨替换 document（`replace_track_notes`）。

use std::time::{SystemTime, UNIX_EPOCH};

use crate::root::Root;
use lumino_event::window::track::{TrackDeletionNote, TrackDeletionPayload};
use lumino_midi_loader::NoteEvent;

impl Root {
    /// 更新音轨列表（从 MIDI 导入）
    /// track_infos: (track_index, track_name, note_count, channel, port)
    pub fn update_tracks(&mut self, track_infos: &[(usize, Option<String>, u64, u8, u8)]) {
        self.sidebar.update_tracks_from_midi(track_infos);
        // 同步视觉位置到文档音轨索引的映射
        // sidebar.tracks 的顺序就是视觉位置，每个 track.id 是文档音轨索引
        self.editor.editor_state.data.track_visual_order =
            self.sidebar.tracks.iter().map(|t| t.id).collect();
    }

    /// 设置编辑器总 ticks
    pub fn set_total_ticks(&mut self, total_ticks: f32) {
        self.editor.set_total_ticks(total_ticks as u32);
        // 同步到走带视图（影响横向滚动最大长度）
        self.arrangement_view.viewport.total_ticks = total_ticks as u32;
    }

    pub fn set_ppq(&mut self, ppq: u16) {
        self.editor.set_ppq(ppq);
        // 按用户当前选择的精度换算 tick，而非无条件重置为四分音符（PPQ）。
        // 旧逻辑每次都把 data 层 snap_precision 硬编码为 ppq，导致：
        //   1) 默认状态下 UI 显示"全音符"而实际吸附是四分音符；
        //   2) 用户手动切换精度后，一旦变更 PPQ，精度会被悄悄改回四分音符。
        // 自定义精度是用户明确指定的绝对 tick 值，PPQ 变更时保持不变。
        if self.toolbar.note_precision != crate::toolbar::NotePrecision::Custom {
            let precision_ticks = self.toolbar.note_precision.as_ticks(ppq);
            self.editor.set_snap_precision(precision_ticks);
            self.editor.set_default_note_length(precision_ticks);
        }
        // 同步到走带视图（影响网格线定位）
        self.arrangement_view.viewport.ppq = ppq;
        // PPQ 变更直接影响小节/拍线位置，必须立即失效网格和标尺缓存
        self.editor.grid_cache.clear();
        self.editor.ruler_cache.clear();
    }

    /// 加载音符到编辑器（整轨替换 document 当前轨）
    /// notes: (tick, key, length, velocity, channel)
    pub fn load_notes(&mut self, notes: &[(f32, u8, f32, u8, u8)]) {
        let track_idx = self.editor.editor_state.data.current_track;
        let events: Vec<NoteEvent> = notes
            .iter()
            .map(|&(tick, key, length, velocity, channel)| {
                NoteEvent::new(
                    tick.max(0.0) as u32,
                    (tick + length).max(0.0) as u32,
                    key,
                    velocity,
                    channel,
                )
            })
            .collect();
        self.editor
            .editor_state
            .data
            .replace_track_notes(track_idx, events);
        // 精确记录受影响音轨（洋葱皮事件级增量）
        self.editor
            .editor_state
            .data
            .mark_track_notes_changed_for(Some(std::collections::HashSet::from([track_idx])));
        self.editor.mark_notes_changed();
    }

    /// 确保 document 包含 sidebar 中全部音轨（新建音轨后同步扩展，幂等）
    ///
    /// 2026-08 修复：`AddTrack` 等只更新 `sidebar.tracks`（UI 列表），
    /// `MidiDocument.notes` 未同步扩轨，新音轨 `insert_note` 越界静默返回
    /// false——音符被丢弃（"只能在第一个音轨放置音符"）。本方法遍历 sidebar
    /// 音轨 id 逐轨 `ensure_track`，保证 document 轨数 ≥ sidebar 最大 id。
    pub fn ensure_sidebar_tracks_in_document(&mut self) {
        let ids: Vec<usize> = self.sidebar.tracks.iter().map(|t| t.id).collect();
        let data = &mut self.editor.editor_state.data;
        for id in ids {
            data.ensure_track(id);
        }
        tracing::debug!(
            "Root: document 音轨已同步（sidebar {} 轨）",
            self.sidebar.tracks.len()
        );
    }

    /// 设置当前音轨
    ///
    /// `open_panel` 控制是否在非 Arrangement 模式下强制打开侧边栏面板：
    /// - `true`：用户手动选轨时，确保面板打开以显示选中音轨
    /// - `false`：MIDI 加载等程序化操作，只刷新数据不强制弹出 UI
    pub fn set_current_track(&mut self, track_idx: usize, open_panel: bool) {
        self.sidebar
            .set_selected_track_with_panel(track_idx, open_panel);
        self.editor.switch_to_track(track_idx);
        self.update_playback_notes();

        // Conductor 轨道自动进入 Tempo 模式，普通轨道切回 Velocity
        let is_conductor = self
            .sidebar
            .tracks
            .first()
            .is_some_and(|t| t.id == track_idx && t.is_conductor);
        let panel = &mut self.editor.velocity_panel;
        if is_conductor {
            if !matches!(panel.edit_mode, crate::editor::velocity::EditMode::Tempo) {
                panel.edit_mode = crate::editor::velocity::EditMode::Tempo;
                tracing::debug!("Root: Conductor 轨道 → Tempo 编辑模式");
            }
        } else if matches!(panel.edit_mode, crate::editor::velocity::EditMode::Tempo) {
            panel.edit_mode = crate::editor::velocity::EditMode::Velocity;
            tracing::debug!("Root: 普通轨道 → Velocity 编辑模式");
        }
    }

    /// 加载指定音轨的音符到编辑器（用于 MIDI 文件，整轨替换 document）
    pub fn load_track_notes(&mut self, track_idx: usize, notes: &[(f32, u8, f32, u8, u8)]) {
        let events: Vec<NoteEvent> = notes
            .iter()
            .map(|&(tick, key, length, velocity, channel)| {
                NoteEvent::new(
                    tick.max(0.0) as u32,
                    (tick + length).max(0.0) as u32,
                    key,
                    velocity,
                    channel,
                )
            })
            .collect();
        self.editor
            .editor_state
            .data
            .replace_track_notes(track_idx, events);
        // 精确记录受影响音轨（洋葱皮事件级增量）
        self.editor
            .editor_state
            .data
            .mark_track_notes_changed_for(Some(std::collections::HashSet::from([track_idx])));

        self.editor.editor_state.data.current_track = track_idx;
        self.editor.mark_notes_changed();
        self.update_playback_notes();
    }

    /// 加载指定音轨的 MIDI 控制事件
    pub fn load_track_midi_events(
        &mut self,
        track_idx: usize,
        events: Vec<crate::playback::MidiTrackEvent>,
    ) {
        if !events.is_empty() {
            self.playback.track_midi_events.insert(track_idx, events);
            tracing::debug!(
                "Root: 音轨 {} 已加载 {} 个 MIDI 控制事件",
                track_idx,
                self.playback
                    .track_midi_events
                    .get(&track_idx)
                    .map_or(0, |v| v.len())
            );
        }
    }

    /// 添加远程音轨（来自协作同步）
    pub fn add_remote_track(&mut self, track_idx: usize) {
        // 确保 sidebar tracks 足够容纳新音轨
        if track_idx >= self.sidebar.tracks.len() {
            self.sidebar.tracks.push(crate::sidebar::Track {
                id: track_idx,
                name: format!("Track {}", track_idx),
                port: 0,
                channel: 0,
                display_label: format!("A{:02}", (track_idx + 1).min(16)),
                is_conductor: false,
                can_delete: true,
                is_muted: false,
                is_soloed: false,
                color: None,
            });
            // 2026-08 修复：同步扩展 document，否则远程音轨无法放置音符
            // （insert_note 越界静默失败，与 AddTrack 同类问题）
            self.editor.editor_state.data.ensure_track(track_idx);
            tracing::info!("协作: 已添加远程音轨 - track_index={}", track_idx);
        } else {
            tracing::warn!("协作: 远程音轨 track_index={} 已存在", track_idx);
        }
    }

    /// 消费 sidebar 中待删除音轨请求，构造 payload 并通过事件通道转发给 Runner
    ///
    /// 由 `handle_sidebar_event` 在 sidebar.update 后调用。pending_track_deletion
    /// 仅携带 track_id——元数据（名称/port/channel）从 `sidebar.tracks` 中查询
    /// （此时 tracks 中该音轨已 remove，故需在 remove 前缓存元数据），
    /// 音符列表从 `EditorData.document` 查询（单一权威源）。
    pub(crate) fn forward_pending_track_deletion(&mut self) {
        let track_id = match self.sidebar.take_pending_track_deletion() {
            Some(id) => id,
            None => return,
        };

        // 元数据缓存（由 sidebar 在删除入口时填充）
        let meta = match self.sidebar.take_pending_track_deletion_meta() {
            Some(m) => m,
            None => {
                tracing::warn!(
                    "forward_pending_track_deletion: 缺少元数据缓存 track_id={}，跳过缓存写入",
                    track_id
                );
                return;
            }
        };

        // 从 EditorData.document 提取音符（单一权威源）
        let notes = self.collect_track_notes_for_deletion(track_id);
        let note_count = notes.len() as u64;
        let max_tick = notes.iter().map(|n| n.end_tick).max().unwrap_or(0);

        // 通道 9（0-indexed）通常为鼓轨
        let is_drum = meta.channel == 9;

        let payload = TrackDeletionPayload {
            track_id: track_id as u16,
            track_name: meta.track_name,
            port: meta.port,
            channel: meta.channel,
            is_drum,
            max_tick,
            original_index: meta.original_index,
            notes,
        };

        tracing::info!(
            "Root: 转发音轨删除请求 track_id={} notes={} → Runner 写入 .lmdeltrack",
            track_id,
            note_count
        );

        lumino_event::emit(lumino_event::Event::Window(
            lumino_event::window::Event::delete_track(payload),
        ));
    }

    /// 消费 sidebar 中"找回删除音轨"对话框打开请求，转发给 Runner
    pub(crate) fn forward_pending_recover_track_dialog(&mut self) {
        if self.sidebar.take_pending_recover_track_dialog() {
            tracing::info!("Root: 请求打开找回删除音轨对话框");
            lumino_event::emit(lumino_event::Event::Window(
                lumino_event::window::Event::open_recover_track_dialog(),
            ));
        }
    }

    /// 从 EditorData.document 提取指定音轨的所有音符（用于删除缓存）
    fn collect_track_notes_for_deletion(&self, track_id: usize) -> Vec<TrackDeletionNote> {
        let data = &self.editor.editor_state.data;
        let source: Vec<NoteEvent> = data.track_notes(track_id).to_vec();

        let mut notes: Vec<TrackDeletionNote> = source
            .into_iter()
            .map(|n| TrackDeletionNote {
                start_tick: n.start_tick,
                end_tick: n.end_tick,
                key: n.key,
                velocity: n.velocity,
                channel: n.channel,
                port: self
                    .sidebar
                    .tracks
                    .iter()
                    .find(|t| t.id == track_id)
                    .map(|t| t.port)
                    .unwrap_or(0),
            })
            .collect();
        // 按 start_tick 排序，便于恢复时直接使用
        notes.sort_by_key(|n| n.start_tick);
        notes
    }

    /// Runner 在扫描缓存目录后调用：把条目列表填充到对话框状态
    pub fn apply_recover_track_entries(
        &mut self,
        entries: Vec<lumino_event::window::track::RecoverTrackEntryPayload>,
    ) {
        let ui_entries: Vec<crate::state::root_state::RecoverTrackEntry> = entries
            .into_iter()
            .map(|e| crate::state::root_state::RecoverTrackEntry {
                path: e.path,
                filename: e.filename,
                track_id: e.track_id,
                track_name: e.track_name,
                port: e.port,
                channel: e.channel,
                note_count: e.note_count,
                deleted_at: e.deleted_at,
                original_index: e.original_index,
            })
            .collect();
        self.set_recover_track_dialog_entries(ui_entries);
    }

    /// Runner 加载 `.lmdeltrack` 后调用：把音轨重新加入 sidebar.tracks + editor_state
    ///
    /// `original_index` 为删除时记录的位置，恢复时优先放回此位置；
    /// 若索引越界则追加到末尾。`reserved_track_ids` 中对应 ID 释放，
    /// 因为音轨重新出现在 sidebar.tracks 中。
    pub fn apply_track_restored(
        &mut self,
        payload: lumino_event::window::track::TrackDeletionPayload,
    ) {
        let track_id = payload.track_id as usize;

        // 构造 sidebar::Track
        let label = crate::sidebar::Sidebar::track_label(payload.port, payload.channel);
        let new_track = crate::sidebar::Track {
            id: track_id,
            name: payload.track_name.clone(),
            port: payload.port,
            channel: payload.channel,
            display_label: label,
            is_conductor: false,
            can_delete: true,
            is_muted: false,
            is_soloed: false,
            color: None,
        };

        // 把音轨放回原位置（若索引越界则追加）
        let insert_idx = payload.original_index.min(self.sidebar.tracks.len());
        self.sidebar.tracks.insert(insert_idx, new_track);

        // 2026-08 修复：恢复轨 id 可能大于当前 document 轨数，先扩轨再写入，
        // 否则 replace_track_notes 越界静默失败，恢复的音符丢失。
        self.editor.editor_state.data.ensure_track(track_id);

        // 恢复音符到 EditorData.document（整轨替换，单一权威源）
        let restored_notes: Vec<NoteEvent> = payload
            .notes
            .iter()
            .map(|n| NoteEvent::new(n.start_tick, n.end_tick, n.key, n.velocity, n.channel))
            .collect();
        self.editor
            .editor_state
            .data
            .replace_track_notes(track_id, restored_notes);
        // 精确记录受影响音轨（洋葱皮事件级增量）
        self.editor
            .editor_state
            .data
            .mark_track_notes_changed_for(Some(std::collections::HashSet::from([track_id])));

        // 释放 reserved_track_id（音轨重新出现，不再占用）
        self.sidebar.release_reserved_track_id(track_id);

        // 如果当前选中音轨已不存在（被删除时 selected_track 可能改向其他音轨），
        // 切换到恢复的音轨
        if !self
            .sidebar
            .tracks
            .iter()
            .any(|t| t.id == self.sidebar.selected_track)
        {
            self.sidebar.selected_track = track_id;
        }

        tracing::info!(
            "Root: 已恢复音轨 track_id={} notes={} → 位置 {}",
            track_id,
            payload.notes.len(),
            insert_idx
        );
    }

    /// Runner 销毁 `.lmdeltrack` 后调用：释放 reserved_track_id
    pub fn apply_track_permanently_deleted(&mut self, track_id: u16) {
        let id = track_id as usize;
        self.sidebar.release_reserved_track_id(id);
        tracing::info!("Root: 已永久销毁音轨缓存 track_id={}，释放 reserved ID", id);
    }

    /// 生成当前时间的 ISO 8601 字符串（用于 deleted_at 字段）
    #[allow(dead_code)]
    fn now_iso8601() -> String {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        // 简化版 ISO 8601：Unix 秒数（足够排序与显示）
        format!("ts:{}", secs)
    }
}
