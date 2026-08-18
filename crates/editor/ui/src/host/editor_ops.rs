//! Host 编辑器操作子模块 - 处理音符和洋葱皮相关操作
//!
//! 子模块组织（保持本文件 < 400 行）：
//! - `clear`: 清空编辑器 / 初始化空白工程
//! - `query`: 音符数据查询（导出 / 计数 / 选中）与动作处理

use crate::host::Host;
use lumino_midi_loader::MidiDocument;
use lumino_note_core::TempoPoint;

mod clear;
mod query;

impl Host {
    /// 重置播放管理器（加载新文件时调用）
    pub fn reset_playback_manager(&mut self) {
        self.root.reset_playback_manager();
    }
    /// 更新音轨列表（从 MIDI 导入）
    /// track_infos: (track_index, track_name, note_count, channel, port)
    pub fn update_tracks(&mut self, track_infos: &[(usize, Option<String>, u64, u8, u8)]) {
        self.root.update_tracks(track_infos);
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
    pub fn set_current_track(&mut self, track_idx: usize, open_panel: bool) {
        self.root.set_current_track(track_idx, open_panel);
        self.window_ctx.window.request_redraw();
    }

    /// 加载指定音轨的音符到编辑器（用于 MIDI 文件）
    /// 这会同时更新当前显示的音符和音轨存储，以便洋葱皮能显示
    pub fn load_track_notes(
        &mut self,
        track_idx: usize,
        notes: &[lumino_midi_loader::TrackNoteView],
    ) {
        self.root.load_track_notes(track_idx, notes);
        // 仅请求重绘，不重建UI树（音符数据由WGPU层处理）
        self.window_ctx.window.request_redraw();
    }

    /// 加载 Tempo 变化事件到播放管理器
    /// tempo_changes: Vec<(tick, tempo_in_microseconds_per_quarter_note)>
    pub fn load_tempo_changes(&mut self, tempo_changes: Vec<(u32, u32)>) {
        self.root.load_tempo_changes(tempo_changes);
    }

    /// 设置 MIDI 文档（独占所有权，2026-08 单一权威源改造）
    ///
    /// 设计意图（见 midi_handler.rs）：
    /// - 文档所有权移入 EditorData.document（唯一权威源），不再保存 Arc 引用
    /// - 当前音轨与其他音轨一律从 document 读取（访问器 current_track_notes / track_notes）
    /// - 洋葱皮渲染通过 build_onion_skin_instances 遍历 document
    ///
    /// 用户硬约束：避免 track_notes + MidiDocument 双份数据共存导致内存暴涨。
    pub fn set_midi_document(&mut self, doc: MidiDocument) {
        // 先读取 tempo/拍号（doc 随后 move 进 root，借用必须在此之前结束）
        let tempo_points: Vec<TempoPoint> = doc
            .tempo_changes
            .iter()
            .map(|&(tick, bpm)| TempoPoint {
                tick: tick as f32,
                bpm: bpm as f64,
            })
            .collect();
        let time_signatures = doc.time_signatures.clone();

        self.root.set_midi_document(doc);
        // 同步 tempo 点到编辑器（用于速度编辑；经统一入口，与 document.tempo_changes 一致）
        self.root
            .editor
            .editor_state
            .data
            .set_tempo_points(tempo_points);
        // 同步拍号变化到编辑器（经统一入口，与 document.time_signatures 一致）
        self.root
            .editor
            .editor_state
            .data
            .set_time_signatures(time_signatures);

        // 新 MIDI 文档加载后必须标记音符数据变化，触发洋葱皮全量重建。
        // 否则 `track_notes_gen` 不变，`OnionSkinState` 认为无需重建，导致
        // 加载后洋葱皮缓冲里仍然是旧文档（或空）数据。
        self.root
            .editor
            .editor_state
            .data
            .mark_track_notes_changed();

        // 拍号/tempo 变化影响网格与标尺，清空缓存强制重建
        self.root.editor.grid_cache.clear();
        self.root.editor.ruler_cache.clear();
        self.render_ctx.render_cache.grid_viewport_hash = 0;
        // 标记音符数据变化，触发走带缓存重建
        self.root.editor.spatial.note_index_dirty.set(true);
        // 注意：不再预加载全量音符到 track_notes——
        // 洋葱皮渲染通过 build_onion_skin_instances 直接遍历 document。
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
}
