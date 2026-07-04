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
    /// track_infos: (track_index, track_name, note_count)
    pub fn update_tracks(&mut self, track_infos: &[(usize, Option<String>, u64)]) {
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
    /// 如果切换到新音轨时旧音轨有脏标记的高精度贴图，
    /// 会立即触发后台重生（绕过冷静期）：
    /// - 用户放置音符后切换音轨 → 立刻开始重生，不必等冷静期到期
    pub fn set_current_track(&mut self, track_idx: usize) {
        // 检查是否从脏音轨切出 → 立即触发重生
        let old_track = self.root.editor.current_track() as u16;
        let old_track_dirty = self.hires_dirty_tracks.contains(&old_track);

        // 执行音轨切换（保存旧音轨 notes 到 track_notes 缓存）
        self.root.set_current_track(track_idx);

        // 如果旧音轨有脏标记，立即触发重生（绕过冷静期）
        if old_track_dirty {
            tracing::info!("音轨切换：旧音轨 {} 有脏标记，立即触发贴图重生", old_track);
            self.force_hires_regen(old_track);
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
        root.editor.spatial.track_note_indices =
            std::cell::RefCell::new(std::collections::HashMap::new());
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

        // UI 缓存
        self.clear_cache();
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
    pub fn handle_action(&mut self, action: message::EditorAction) {
        let track_idx = self.root.editor.current_track() as u16;
        self.root.handle_editor_action(action);
        // 编辑动作可能改变音符 → 标记当前音轨高精度贴图为脏
        self.mark_hires_dirty(track_idx);
        // 仅请求重绘，不重建UI树（编辑器动作由canvas/WGPU层处理）
        self.window_ctx.window.request_redraw();
    }
}
