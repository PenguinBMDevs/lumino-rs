//! Host 编辑器操作子模块 - 处理音符和洋葱皮相关操作

use crate::host::{Host, types::NoteData};
use crate::message;
use lumino_core::TempoPoint;
use lumino_midi_loader::MidiDocument;
use std::sync::Arc;

impl Host {
    /// 重置播放管理器（加载新文件时调用）
    pub fn reset_playback_manager(&mut self) {
        self.root.reset_playback_manager();
    }
    /// 更新音轨列表（从 MIDI 导入）
    /// track_infos: (track_index, track_name, note_count, channel)
    pub fn update_tracks(&mut self, track_infos: &[(usize, Option<String>, u64, u8)]) {
        self.root.update_tracks(track_infos);
        // 仅请求重绘，不重建UI树（音轨列表数据由WGPU层处理）
        self.window_ctx.window.request_redraw();
    }

    /// 设置编辑器总 ticks
    pub fn set_total_ticks(&mut self, total_ticks: f32) {
        self.root.set_total_ticks(total_ticks);
        self.root.editor.grid_cache.clear();
        self.root.editor.ruler_cache.clear();
        self.render_ctx.render_cache.grid_viewport_hash = 0;
        // 仅请求重绘，不重建UI树（网格线数据由WGPU层处理）
        self.window_ctx.window.request_redraw();
    }

    /// 设置编辑器的 PPQ (division)
    pub fn set_ppq(&mut self, ppq: u16) {
        self.root.set_ppq(ppq);
        self.root.editor.grid_cache.clear();
        self.root.editor.ruler_cache.clear();
        self.render_ctx.render_cache.grid_viewport_hash = 0;
        self.render_ctx.render_cache.note_viewport_hash = 0;
        self.render_ctx.render_cache.note_render_viewport = None;
        // 仅请求重绘，不重建UI树（网格线数据由WGPU层处理）
        self.window_ctx.window.request_redraw();
    }

    /// 加载音符到编辑器
    /// notes: (tick, key, length, velocity, channel)
    pub fn load_notes(&mut self, notes: &[(f32, u8, f32, u8, u8)]) {
        self.root.load_notes(notes);
        // 仅请求重绘，不重建UI树（音符数据由WGPU层处理）
        self.window_ctx.window.request_redraw();
    }

    /// 设置当前音轨
    ///
    /// 如果切换到新音轨时旧音轨有脏标记的高精度贴图：
    /// 1. 先发送临时脏区域覆层命令，在旧音轨位置立即显示编辑内容；
    /// 2. 执行音轨切换；
    /// 3. 立即触发后台重生（绕过冷静期），重生完成后自动替换覆层。
    ///
    /// 覆层与重生均以音轨组（track_group）为单位合并处理，
    /// 避免同组多个脏音轨互相覆盖或重生成时丢失同组其他音轨数据。
    pub fn set_current_track(&mut self, track_idx: usize, open_panel: bool) {
        // 收集当前所有脏音轨，避免切换时只显示单个音轨的覆盖层
        let dirty_tracks: Vec<u16> = self.hires_dirty_tracks.iter().copied().collect();
        let old_track = self.root.editor.current_track() as u16;
        tracing::debug!(
            "[onion-dirty] set_current_track: old_track={}, new_track={}, dirty_tracks={:?}",
            old_track,
            track_idx,
            dirty_tracks
        );

        // 切换前先发送临时脏区域覆层，确保用户立刻看到所有刚编辑的音符
        let context_ready = self.hires_config.is_some()
            && self.hires_midi_hash.is_some()
            && self.hires_gen_info.is_some();
        tracing::debug!(
            "[onion-dirty] context_ready={}, config={}, hash={}, gen_info={}",
            context_ready,
            self.hires_config.is_some(),
            self.hires_midi_hash.is_some(),
            self.hires_gen_info.is_some()
        );

        if let (Some(cfg), Some(hash), Some((ppq, key_count, total_ticks))) = (
            self.hires_config.clone(),
            self.hires_midi_hash.clone(),
            self.hires_gen_info,
        ) {
            // 按音轨组分组脏音轨，同组只发送一个合并覆层
            let mut dirty_by_group: std::collections::HashMap<u32, Vec<u16>> =
                std::collections::HashMap::new();
            for &dirty_track in &dirty_tracks {
                let group = (dirty_track / lumino_gfx::TRACKS_PER_GROUP) as u32;
                dirty_by_group.entry(group).or_default().push(dirty_track);
            }

            // 推断需要的音轨总数
            let max_dirty = dirty_tracks.iter().copied().max().unwrap_or(0);
            let track_count = (self.root.sidebar.tracks.len() as u16)
                .max(max_dirty + 1)
                .max(track_idx as u16 + 1);

            for (group, tracks) in &dirty_by_group {
                let group_start = (group * lumino_gfx::TRACKS_PER_GROUP as u32) as u16;
                let group_end = (group_start + lumino_gfx::TRACKS_PER_GROUP).min(track_count);
                let mut group_notes = Vec::with_capacity((group_end - group_start) as usize);
                for t in group_start..group_end {
                    // 脏音轨使用快照（当前帧可能尚未保存到 track_notes）
                    let notes = if let Some(dirty_notes) = self.hires_dirty_regions.get(&t) {
                        dirty_notes.clone()
                    } else {
                        self.get_track_notes_for_hires(t)
                    };
                    group_notes.push(notes);
                }

                let representative = tracks[0];
                tracing::debug!(
                    "[onion-dirty] 发送 ShowHiResDirtyOverlay: representative_track={}, group={}, group_tracks={}",
                    representative,
                    group,
                    group_notes.len()
                );
                if group_notes.iter().any(|n| !n.is_empty()) {
                    self.send_hires_dirty_overlay(lumino_gfx::render_thread::HiResTrackParams {
                        track_idx: representative,
                        group_notes,
                        ppq,
                        key_count,
                        total_ticks,
                        track_count,
                        config: cfg.clone(),
                        midi_hash: hash.clone(),
                    });
                }
            }
        }

        // 执行音轨切换（保存旧音轨 notes 到 track_notes 缓存）
        self.root.set_current_track(track_idx, open_panel);

        // 按音轨组触发后台重生，每个 group 只重生一次
        let mut regen_groups = std::collections::HashSet::new();
        for &dirty_track in &dirty_tracks {
            let group = (dirty_track / lumino_gfx::TRACKS_PER_GROUP) as u32;
            if regen_groups.insert(group) {
                tracing::debug!(
                    "[onion-dirty] 触发 force_hires_regen: track={}",
                    dirty_track
                );
                self.force_hires_regen(dirty_track);
            }
        }

        // 仅请求重绘，不重建UI树（音轨切换由WGPU层处理）
        self.window_ctx.window.request_redraw();
    }

    /// 加载指定音轨的音符到编辑器（用于 MIDI 文件）
    /// 这会同时更新当前显示的音符和音轨存储，以便洋葱皮能显示
    pub fn load_track_notes(&mut self, track_idx: usize, notes: &[(f32, u8, f32, u8, u8)]) {
        self.root.load_track_notes(track_idx, notes);
        // 仅请求重绘，不重建UI树（音符数据由WGPU层处理）
        self.window_ctx.window.request_redraw();
    }

    /// 加载 Tempo 变化事件到播放管理器
    /// tempo_changes: Vec<(tick, tempo_in_microseconds_per_quarter_note)>
    pub fn load_tempo_changes(&mut self, tempo_changes: Vec<(u32, u32)>) {
        self.root.load_tempo_changes(tempo_changes);
    }

    /// 设置 MIDI 文档引用（供懒加载非当前音轨的音符使用）
    pub fn set_midi_document(&mut self, doc: Arc<MidiDocument>) {
        self.root.set_midi_document(doc.clone());
        // 同步 tempo 点到编辑器（用于速度编辑）
        self.root.editor.editor_state.data.tempo_points = doc
            .tempo_changes
            .iter()
            .map(|&(tick, bpm)| TempoPoint {
                tick: tick as f32,
                bpm: bpm as f64,
            })
            .collect();
        self.root.editor.editor_state.data.document = Some(doc);
        // 标记音符数据变化，触发走带缓存重建
        self.root.editor.spatial.note_index_dirty.set(true);
    }

    /// 加载音轨 MIDI 控制事件（CC/PC/PB）
    pub fn load_track_midi_events(
        &mut self,
        track_idx: usize,
        events: Vec<crate::playback::MidiTrackEvent>,
    ) {
        self.root.load_track_midi_events(track_idx, events);
    }

    /// 设置播放用 MIDI 输出连接
    pub fn set_playback_midi_output(&mut self, output: Box<dyn lumino_midi_io::OutputConnection>) {
        self.root.set_midi_output(output);
    }

    /// 清除播放用 MIDI 输出连接
    pub fn clear_playback_midi_output(&mut self) {
        self.root.clear_midi_output();
    }

    /// 设置 MIDI API（用于录制等需要输入的功能）
    pub fn set_midi_api(&mut self, api: Box<dyn lumino_midi_io::Api>) {
        self.root.set_midi_api(api);
    }

    /// 播放管理器是否已初始化
    pub fn has_playback_manager(&self) -> bool {
        self.root.playback.manager.is_some()
    }

    /// 检查是否正在播放
    pub fn is_playing(&self) -> bool {
        self.root.is_playing()
    }

    /// 清空编辑器（用于新建工程 / 关闭文件）
    ///
    /// 释放所有编辑器内存（音符数据、历史记录、空间索引、MIDI 事件、文档引用）
    /// 并还原编辑器状态到默认（视图滚动/缩放、工具、侧边栏、播放管理器）。
    pub fn clear_editor(&mut self) {
        let root = &mut self.root;

        // 使用 EditorState::reset() 统一重置核心状态
        root.editor.editor_state.reset();

        // 编辑器私有内部状态（播放位置等）
        root.editor.velocity_panel.edit_mode = crate::editor::velocity::EditMode::Tempo;
        root.editor.reset_internal_state();

        // 空间索引（惰性重建）
        root.editor.spatial.note_index = std::cell::RefCell::new(None);
        root.editor.spatial.note_index_dirty = std::cell::Cell::new(true);
        root.editor.spatial.query_cache = std::cell::RefCell::new(Vec::new());

        // MIDI 控制事件
        root.playback.track_midi_events.clear();

        // 协作远端光标
        root.editor.remote_cursors.clear();

        // 失效所有 Canvas 缓存
        root.editor.grid_cache.clear();
        root.editor.keyboard_cache.clear();
        root.editor.ruler_cache.clear();

        // 播放管理器
        root.reset_playback_manager();
        root.playback.pending_tempo_changes = None;
        root.playback.pending_midi_output = None;
        root.toolbar.is_playing = false;

        // 侧边栏音轨列表
        root.sidebar.tracks.clear();
        root.sidebar.selected_track = 0;

        // RenderCache 视口哈希失效（强制重建 GPU 实例）
        self.render_ctx.render_cache.grid_viewport_hash = 0;
        self.render_ctx.render_cache.note_viewport_hash = 0;
        self.render_ctx.render_cache.note_render_viewport = None;

        // UI 缓存
        self.clear_cache();

        // 清空后重新初始化默认高精度洋葱皮上下文，确保后续编辑仍能生成贴图
        self.init_default_hires_context();

        self.window_ctx.window.request_redraw();
        tracing::info!("UI: 编辑器已完全清空（含历史记录、空间索引、播放状态）");
    }

    /// 获取编辑器中的所有音符数据（用于保存）
    ///
    /// 返回 (track_idx, notes) 列表，其中 notes 格式为 (tick, key, length, velocity, channel)
    pub fn get_editor_notes(&self) -> Vec<(usize, Vec<NoteData>)> {
        let mut result = Vec::new();

        // 先保存当前音轨的音符
        if !self.root.editor.editor_state.data.notes.is_empty() {
            let current_notes: Vec<NoteData> = self
                .root
                .editor
                .editor_state
                .data
                .notes
                .iter()
                .map(|n| (n.tick, n.key as u8, n.length, n.velocity, n.channel))
                .collect();
            result.push((
                self.root.editor.editor_state.data.current_track,
                current_notes,
            ));
        }

        // 添加其他音轨的音符
        for (&track_idx, notes) in &self.root.editor.editor_state.data.track_notes {
            if track_idx != self.root.editor.editor_state.data.current_track {
                let track_notes: Vec<NoteData> = notes
                    .iter()
                    .map(|n| (n.tick, n.key as u8, n.length, n.velocity, n.channel))
                    .collect();
                result.push((track_idx, track_notes));
            }
        }

        result
    }

    /// 获取编辑器中的音符数量（用于判断是否有内容）
    pub fn get_editor_note_count(&self) -> usize {
        let current_count = self.root.editor.editor_state.data.notes.len();
        let track_notes_count: usize = self
            .root
            .editor
            .editor_state
            .data
            .track_notes
            .values()
            .map(|v| v.len())
            .sum();
        current_count + track_notes_count
    }

    /// 检查音符数据是否已变化
    pub fn has_notes_changed(&self) -> bool {
        self.root.editor.notes_changed()
    }

    /// 获取并清空待处理的音频动作
    pub fn take_audio_actions(&mut self) -> Vec<message::AudioAction> {
        self.root.take_audio_actions()
    }

    /// 处理编辑器动作
    ///
    /// 仅在音符数据确实发生变化时才标记当前音轨高精度贴图为脏。
    /// 先按动作类型过滤：只有可能修改音符的动作才检查 `notes_changed()`，
    /// 避免 Moved/Released/Copy/SelectAll 等不会改音符的动作被误判为脏音轨。
    pub fn handle_action(&mut self, action: message::EditorAction) {
        let track_idx = self.root.editor.current_track() as u16;
        tracing::debug!(
            "[onion-dirty] Host::handle_action: action={:?}, track={}",
            action,
            track_idx
        );

        // 先确定该动作是否可能修改音符数据
        // 确定会改：Delete/Cut/Paste → 直接标记脏，不问 notes_changed
        // 可能改：Pressed/Released/DoubleClicked/Undo/Redo → 依赖 notes_changed 判断
        // 绝不会改：Moved/Copy/SelectAll/Scrubbed/Scrolled/IndicatorDrag → 跳过
        let is_definite_mutation = matches!(
            action,
            message::EditorAction::DeletePressed
                | message::EditorAction::Cut
                | message::EditorAction::Paste
        );
        let is_possible_mutation = matches!(
            action,
            message::EditorAction::Pressed { .. }
                | message::EditorAction::Released
                | message::EditorAction::DoubleClicked(_)
                | message::EditorAction::Undo
                | message::EditorAction::Redo
        );

        let notes_changed = self.root.handle_editor_action(action);
        if is_definite_mutation || (is_possible_mutation && notes_changed) {
            // 编辑动作确实改变了音符 → 标记当前音轨高精度贴图为脏
            self.mark_hires_dirty(track_idx);
        }
        // 仅请求重绘，不重建UI树（编辑器动作由canvas/WGPU层处理）
        self.window_ctx.window.request_redraw();
    }
}
