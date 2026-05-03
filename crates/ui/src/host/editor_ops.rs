//! Host 编辑器操作子模块 - 处理音符和洋葱皮相关操作

use crate::host::{Host, types::NoteData};
use crate::{editor::note::Note, message};
use lumino_core::midi::MidiDocument;
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
    pub fn set_current_track(&mut self, track_idx: usize) {
        self.root.set_current_track(track_idx);
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

    /// 预加载音轨音符到 track_notes（仅用于洋葱皮，不显示）
    ///
    /// # 参数
    /// * `track_idx` - 音轨索引
    /// * `notes` - 音符列表，格式为 (tick, key, length, velocity, channel)
    pub fn load_track_notes_for_onion_skin(
        &mut self,
        track_idx: usize,
        notes: &[(f32, u8, f32, u8, u8)],
    ) {
        tracing::debug!(
            "UI::load_track_notes_for_onion_skin: track_idx={}, notes_count={}",
            track_idx,
            notes.len()
        );

        // 直接保存到 editor.track_notes，不更新当前显示
        let mut track_notes: im::Vector<Note> = im::Vector::new();
        for (tick, key, length, velocity, channel) in notes {
            let editor_key = *key as u16;
            track_notes.push_back(
                Note::new(*tick, editor_key, *length)
                    .with_velocity(*velocity)
                    .with_channel(*channel),
            );
        }

        if !track_notes.is_empty() {
            self.root
                .editor
                .editor_state
                .data
                .track_notes
                .insert(track_idx, track_notes);
            self.root.invalidate_onion_skin_cache();
        }

        // 不需要重绘，因为这些音符是用于洋葱皮的，不是当前显示的
    }

    /// 加载 Tempo 变化事件到播放管理器
    /// tempo_changes: Vec<(tick, tempo_in_microseconds_per_quarter_note)>
    pub fn load_tempo_changes(&mut self, tempo_changes: Vec<(u32, u32)>) {
        self.root.load_tempo_changes(tempo_changes);
    }

    /// 设置 MIDI 文档引用（供懒加载非当前音轨的音符使用）
    pub fn set_midi_document(&mut self, doc: Arc<MidiDocument>) {
        self.root.set_midi_document(doc.clone());
        // 同步到 Editor，供 ensure_track_notes_loaded 使用
        self.root.editor.editor_state.data.document = Some(doc);
    }

    /// 加载音轨 MIDI 控制事件（CC/PC/PB）
    pub fn load_track_midi_events(
        &mut self,
        track_idx: usize,
        events: Vec<crate::playback::MidiTrackEvent>,
    ) {
        self.root.load_track_midi_events(track_idx, events);
    }

    /// 预加载音轨 MIDI 控制事件到洋葱皮缓存
    pub fn load_track_midi_events_for_onion_skin(
        &mut self,
        track_idx: usize,
        events: Vec<crate::playback::MidiTrackEvent>,
    ) {
        self.root
            .load_track_midi_events_for_onion_skin(track_idx, events);
    }

    /// 设置播放用 MIDI 输出连接
    pub fn set_playback_midi_output(&mut self, output: Box<dyn lumino_midi::OutputConnection>) {
        self.root.set_midi_output(output);
    }

    /// 清除播放用 MIDI 输出连接
    pub fn clear_playback_midi_output(&mut self) {
        self.root.clear_midi_output();
    }

    /// 播放管理器是否已初始化
    pub fn has_playback_manager(&self) -> bool {
        self.root.playback_manager.is_some()
    }

    /// 检查是否正在播放
    pub fn is_playing(&self) -> bool {
        self.root.is_playing()
    }

    // ════════════════════════════════════════════════════════════════════════════
    // 洋葱皮 API
    // ════════════════════════════════════════════════════════════════════════════

    /// 启用洋葱皮功能
    pub fn enable_onion_skin(&mut self) {
        self.root.editor.enable_onion_skin();
        self.root.invalidate_onion_skin_cache();
        self.window_ctx.window.request_redraw();
    }

    /// 禁用洋葱皮功能
    pub fn disable_onion_skin(&mut self) {
        self.root.editor.disable_onion_skin();
        self.root.invalidate_onion_skin_cache();
        self.window_ctx.window.request_redraw();
    }

    /// 切换洋葱皮开关状态
    pub fn toggle_onion_skin(&mut self) {
        self.root.editor.toggle_onion_skin();
        self.root.invalidate_onion_skin_cache();
        self.window_ctx.window.request_redraw();
    }

    /// 检查洋葱皮是否启用
    pub fn is_onion_skin_enabled(&self) -> bool {
        self.root.editor.is_onion_skin_enabled()
    }

    /// 设置音轨的洋葱皮 RGB 颜色（透明度保持不变）
    ///
    /// # 参数
    /// * `track_idx` - 音轨索引
    /// * `r`, `g`, `b` - RGB 颜色分量 (0.0 - 1.0)
    pub fn set_onion_skin_color_rgb(&mut self, track_idx: usize, r: f32, g: f32, b: f32) {
        let alpha = self.root.editor.onion_skin_opacity();
        self.root
            .editor
            .set_onion_skin_color(track_idx, iced_core::Color::from_rgba(r, g, b, alpha));
        self.root.invalidate_onion_skin_cache();
        self.window_ctx.window.request_redraw();
    }

    pub fn set_onion_skin_color_rgba(&mut self, track_idx: usize, r: f32, g: f32, b: f32, a: f32) {
        self.root
            .editor
            .set_onion_skin_color(track_idx, iced_core::Color::from_rgba(r, g, b, a));
        self.root.invalidate_onion_skin_cache();
        self.window_ctx.window.request_redraw();
    }

    /// 获取音轨的洋葱皮颜色
    ///
    /// 返回 (r, g, b, a) 元组
    pub fn get_onion_skin_color(&self, track_idx: usize) -> (f32, f32, f32, f32) {
        let color = self.root.editor.get_onion_skin_color(track_idx);
        (color.r, color.g, color.b, color.a)
    }

    /// 设置洋葱皮透明度
    ///
    /// # 参数
    /// * `opacity` - 透明度值，范围 0.0（完全透明）到 1.0（完全不透明）
    pub fn set_onion_skin_opacity(&mut self, opacity: f32) {
        self.root.editor.set_onion_skin_opacity(opacity);
        self.root.invalidate_onion_skin_cache();
        self.window_ctx.window.request_redraw();
    }

    /// 获取洋葱皮透明度
    pub fn onion_skin_opacity(&self) -> f32 {
        self.root.editor.onion_skin_opacity()
    }

    /// 设置是否显示所有音轨的洋葱皮
    pub fn set_onion_skin_show_all(&mut self, show_all: bool) {
        self.root.editor.set_onion_skin_show_all(show_all);
        self.root.invalidate_onion_skin_cache();
        self.window_ctx.window.request_redraw();
    }

    /// 添加音轨到洋葱皮显示列表
    pub fn add_onion_skin_track(&mut self, track_idx: usize) {
        self.root.editor.add_onion_skin_track(track_idx);
        self.root.invalidate_onion_skin_cache();
        self.window_ctx.window.request_redraw();
    }

    /// 从洋葱皮显示列表移除音轨
    pub fn remove_onion_skin_track(&mut self, track_idx: usize) {
        self.root.editor.remove_onion_skin_track(track_idx);
        self.root.invalidate_onion_skin_cache();
        self.window_ctx.window.request_redraw();
    }

    /// 清空编辑器（用于新建工程）
    pub fn clear_editor(&mut self) {
        self.root.editor.editor_state.data.notes.clear();
        self.root.editor.editor_state.data.track_notes.clear();
        self.root.editor.editor_state.data.current_track = 0;
        self.root.editor.grid_cache.clear();
        // 释放 MIDI 文档引用（Arc），让大块事件内存可以被回收
        self.root.editor.editor_state.data.document = None;
        self.root.midi_document = None;
        self.root.cached_onion_skin_notes = None;
        self.clear_cache();
        // 仅请求重绘，不重建UI树（编辑器清空由WGPU层处理）
        self.window_ctx.window.request_redraw();
        tracing::info!("UI: 编辑器已清空");
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

    /// 获取并清空待处理的音频动作
    pub fn take_audio_actions(&mut self) -> Vec<message::AudioAction> {
        self.root.take_audio_actions()
    }

    /// 处理编辑器动作
    pub fn handle_action(&mut self, action: message::EditorAction) {
        self.root.handle_editor_action(action);
        // 仅请求重绘，不重建UI树（编辑器动作由canvas/WGPU层处理）
        self.window_ctx.window.request_redraw();
    }
}
