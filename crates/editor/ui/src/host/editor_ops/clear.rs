//! Host 编辑器清空与空白工程初始化

use crate::host::Host;
use lumino_midi_loader::MidiDocument;

impl Host {
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

        // 清空后工程处于干净状态（无未保存更改），避免误弹保存确认对话框
        self.mark_project_clean();

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

        // 空白工程为干净状态（无未保存更改），避免误弹保存确认对话框
        self.mark_project_clean();
    }
}
