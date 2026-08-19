//! 编辑器操作 - 音轨管理
//!
//! 2026-08 单一权威源改造：音符数据唯一权威是 `EditorData.document`，
//! 加载/删除/恢复音轨直接整轨替换 document（`replace_track_notes`）。
//!
//! 子模块组织（保持本文件 < 400 行）：
//! - `deletion`: 音轨删除/找回/恢复（转发 Runner、缓存写入、恢复流程）

use crate::root::Root;
use lumino_midi_loader::NoteEvent;

mod deletion;

impl Root {
    /// 更新音轨列表（从 MIDI 导入）
    /// track_infos: (track_index, track_name, note_count, channel, port)
    pub fn update_tracks(&mut self, track_infos: &[(usize, Option<String>, u64, u8, u8)]) {
        self.sidebar.update_tracks_from_midi(track_infos);
        // 同步视觉位置到文档音轨索引的映射
        // sidebar.tracks 的顺序就是视觉位置，每个 track.id 是文档音轨索引
        self.sync_track_visual_order();
    }

    /// 同步视觉位置 → 文档音轨索引 映射（`track_visual_order`）
    ///
    /// 侧边栏音轨顺序（视觉顺序）变化时调用：拖拽排序、新增音轨、删除音轨、
    /// 恢复音轨。映射用于走带交互层把视觉位置（行索引）转换为 document
    /// 音轨索引，保证排序后点击/框选/移动/擦除/切割落在正确的音轨上。
    ///
    /// 幂等，O(n)。仅在音轨结构变化时调用（避免每帧 6 万轨开销）。
    pub fn sync_track_visual_order(&mut self) {
        self.editor.editor_state.data.track_visual_order =
            self.sidebar.tracks.iter().map(|t| t.id).collect();
    }

    /// 设置编辑器总 ticks
    pub fn set_total_ticks(&mut self, total_ticks: f32) {
        self.editor.set_total_ticks(total_ticks as u32);
        // 同步到走带视图（影响横向滚动最大长度）
        self.arrangement_view.viewport.total_ticks = total_ticks as u32;
    }

    /// 设置每四分音符的时钟数（PPQ）
    pub fn set_ppq(&mut self, ppq: u16) {
        self.editor.set_ppq(ppq);
        // 同步到单一权威源 document 的 division，保证保存/导出工程时
        // 写入用户设置的 PPQ（此前只更新视图状态，工程文件永远落盘旧值 480）。
        if let Some(doc) = self.editor.editor_state.data.document.as_mut() {
            doc.division = ppq;
        }
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

        // Conductor 轨道自动进入 Tempo 模式，普通轨道切回 Velocity。
        // 按 id 查找（拖动排序后 conductor 可能不在首位）
        let is_conductor = self
            .sidebar
            .tracks
            .iter()
            .find(|t| t.id == track_idx)
            .is_some_and(|t| t.is_conductor);
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
    pub fn load_track_notes(
        &mut self,
        track_idx: usize,
        notes: &[lumino_midi_loader::TrackNoteView],
    ) {
        let events: Vec<NoteEvent> = notes
            .iter()
            .map(|n| {
                NoteEvent::new(
                    n.start_tick.max(0.0) as u32,
                    (n.start_tick + n.length).max(0.0) as u32,
                    n.key,
                    n.velocity,
                    n.channel,
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
}
