//! Host 编辑器操作子模块 - 处理音符和洋葱皮相关操作

use crate::host::{Host, types::NoteData};
use crate::message;
use lumino_midi_loader::MidiDocument;
use lumino_note_core::TempoPoint;

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
        // 同步 tempo 点到编辑器（用于速度编辑）
        self.root.editor.editor_state.data.tempo_points = tempo_points;
        // 同步拍号变化到编辑器
        self.root.editor.editor_state.data.time_signatures = time_signatures;

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

    /// 清空编辑器（用于新建工程 / 关闭文件）
    ///
    /// 释放所有编辑器内存（音符数据、历史记录、空间索引、MIDI 事件、文档引用）
    /// 并还原编辑器状态到默认（视图滚动/缩放、工具、侧边栏、播放管理器）。
    pub fn clear_editor(&mut self) {
        let root = &mut self.root;

        // 使用 EditorState::reset() 统一重置核心状态
        root.editor.editor_state.reset();

        // 工程设置（标题/作者/版权/BPM/拍号）属于工程级数据，
        // 关闭工程/新建工程后必须恢复默认值，不得残留到下一个工程
        root.reset_project_settings();

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

        // 初始化空白工程：空 document（默认 2 轨 Conductor + Setup）+ 重建 sidebar.tracks。
        // 2026-08 修复：此前 reset() 置 document=None 且 sidebar.tracks 清空，
        // 导致空白工程创建音符被三重拦截（track_count=0 / document=None / current_track=0）。
        self.init_blank_project();

        // UI 缓存
        self.clear_cache();

        // 清空后重新初始化默认高精度洋葱皮上下文，确保后续编辑仍能生成贴图
        self.init_default_waterfall_context();

        // 清空高精度脏标记，避免新建工程/关闭文件后残留脏状态误触发
        self.waterfall_dirty_tracks.clear();

        self.window_ctx.window.request_redraw();
        tracing::info!("UI: 编辑器已完全清空（含历史记录、空间索引、播放状态）");
    }

    /// 初始化空白工程（新建文件 / 关闭文件 / 应用启动后调用）。
    ///
    /// 2026-08 修复：此前 `clear_editor` 清空 `sidebar.tracks` 且 `reset()`
    /// 置 `document = None`，导致空白工程创建音符被三重拦截
    /// （`track_count=0` / `document=None` / `current_track==0` 均失败）。
    /// 本方法重建空 `MidiDocument`（默认 2 轨：Conductor + Setup）并同步
    /// sidebar.tracks，使空白工程立即可编辑。
    ///
    /// 应用启动路径（`WindowManager::new`）也会调用本方法，保证
    /// "启动后直接画音符"验收链路成立（根因：`EditorData::new` 时
    /// `document = None` + `current_track = 0`，铅笔音符被 `current_track == 0`
    /// 拦截）。幂等：重复调用每次都重建 2 轨空白文档。
    pub fn init_blank_project(&mut self) {
        // 空白文档的 division 取编辑器当前 PPQ（默认 1920），
        // 与视图状态保持一致，保证新工程保存的 PPQ 正确（不再硬编码 480）。
        let doc = MidiDocument::empty_with_tracks(2, self.root.editor.editor_state.view.ppq);

        // 空轨道信息（名称/音符数=0/通道/端口），与 midi_handler 导入路径一致
        let track_infos: Vec<(usize, Option<String>, u64, u8, u8)> = (0..doc.track_count())
            .map(|i| {
                (
                    i,
                    doc.track_name(i).map(|s| s.to_string()),
                    doc.track_note_count(i as u16),
                    doc.track_channel(i as u16),
                    doc.track_port(i as u16),
                )
            })
            .collect();

        // 先重建 sidebar.tracks（update_tracks 内部同步 track_visual_order）
        self.update_tracks(&track_infos);
        // 再将空文档设为当前文档（set_midi_document 内部同步 tempo/拍号等）
        self.set_midi_document(doc);

        // 将当前轨道切到一条可编辑轨（跳过 Conductor 轨道 0，insert_note_at_tick 限制）
        let editable = self
            .root
            .sidebar
            .tracks
            .iter()
            .find(|t| !t.is_conductor)
            .map(|t| t.id)
            .unwrap_or(1);
        self.root.editor.editor_state.data.current_track = editable;
        self.root.sidebar.selected_track = editable;
        tracing::info!(
            "UI: 空白工程已初始化（document 2 轨，当前轨 {} 可编辑）",
            editable
        );
    }

    /// 获取编辑器中的所有音符数据（用于保存）
    ///
    /// 返回 (track_idx, notes) 列表，其中 notes 格式为 (tick, key, length, velocity, channel)。
    /// 单一权威源：音符一律从 document 读取（2026-08 改造）。
    pub fn get_editor_notes(&self) -> Vec<(usize, Vec<NoteData>)> {
        let mut result = Vec::new();
        let Some(doc) = self.root.editor.editor_state.data.document.as_ref() else {
            return result;
        };
        for track_idx in 0..doc.track_count() {
            let notes = doc.track_notes(track_idx);
            if notes.is_empty() {
                continue;
            }
            let track_notes: Vec<NoteData> = notes
                .iter()
                .map(|n| {
                    (
                        n.start_tick as f32,
                        n.key,
                        (n.end_tick - n.start_tick) as f32,
                        n.velocity,
                        n.channel,
                    )
                })
                .collect();
            result.push((track_idx, track_notes));
        }
        result
    }

    /// 获取编辑器中的音符数量（用于判断是否有内容）
    pub fn get_editor_note_count(&self) -> usize {
        let Some(doc) = self.root.editor.editor_state.data.document.as_ref() else {
            return 0;
        };
        (0..doc.track_count())
            .map(|track_idx| doc.track_notes(track_idx).len())
            .sum()
    }

    /// 获取当前选中的音符（用于"导出为素材"）
    ///
    /// - 卷帘模式：当前音轨的选中音符索引（`selected_notes` / `selection_bitset`）；
    /// - 走带模式：`arrange_selection` 跨音轨矩形框选覆盖的音符。
    ///
    /// 返回 `(track_idx, [(tick, key, length, velocity, channel)])`（仅含选中音符的音轨）。
    pub fn get_selected_notes(&self) -> Vec<(usize, Vec<NoteData>)> {
        let mut result: Vec<(usize, Vec<NoteData>)> = Vec::new();
        let Some(doc) = self.root.editor.editor_state.data.document.as_ref() else {
            return result;
        };
        let data = &self.root.editor.editor_state.data;

        // 卷帘模式：当前轨选中音符索引
        if self.root.editor.has_selection() {
            let indices = self.root.editor.get_selected_indices();
            let notes = doc.track_notes(data.current_track);
            let selected: Vec<NoteData> = indices
                .into_iter()
                .filter_map(|idx| notes.get(idx))
                .map(|n| {
                    (
                        n.start_tick as f32,
                        n.key,
                        (n.end_tick - n.start_tick) as f32,
                        n.velocity,
                        n.channel,
                    )
                })
                .collect();
            if !selected.is_empty() {
                result.push((data.current_track, selected));
            }
            return result;
        }

        // 走带模式：跨音轨矩形框选
        let arrangement = &data.arrange_selection;
        if !arrangement.is_empty() {
            for track_idx in 0..doc.track_count() {
                let notes = doc.track_notes(track_idx);
                let selected: Vec<NoteData> = notes
                    .iter()
                    .filter(|n| arrangement.contains(track_idx as u16, n.start_tick, n.key))
                    .map(|n| {
                        (
                            n.start_tick as f32,
                            n.key,
                            (n.end_tick - n.start_tick) as f32,
                            n.velocity,
                            n.channel,
                        )
                    })
                    .collect();
                if !selected.is_empty() {
                    result.push((track_idx, selected));
                }
            }
        }
        result
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
    /// 仅在音符数据确实发生变化时才标记当前音轨贴图瀑布流为脏。
    /// 先按动作类型过滤：只有可能修改音符的动作才检查 `notes_changed()`，
    /// 避免 Moved/Released/Copy/SelectAll 等不会改音符的动作被误判为脏音轨。
    pub fn handle_action(&mut self, action: message::EditorAction) {
        puffin::profile_function!();
        let track_idx = self.root.editor.current_track() as u16;

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
            // 编辑动作确实改变了音符 → 标记当前音轨贴图瀑布流为脏
            self.mark_waterfall_dirty(track_idx);
        }
        // 仅请求重绘，不重建UI树（编辑器动作由canvas/WGPU层处理）
        self.window_ctx.window.request_redraw();
    }
}
