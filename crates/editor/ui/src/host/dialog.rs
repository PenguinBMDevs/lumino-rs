//! Host 对话框和协作子模块 - 处理对话框状态和远程协作
//!
//! 视频导出相关方法见 `video` 子模块。

mod video;

use crate::host::{Host, types::DialogResult};
use crate::state::root_state::CollaborationViewState;
use crate::{message, window};

impl Host {
    /// 设置加载确认对话框（用于独立对话框窗口）
    pub fn set_load_confirm_dialog(&mut self, file_path: &str, size_mb: f64) {
        self.root.set_load_confirm_dialog(file_path, size_mb);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 设置自定义精度对话框是否打开（用于独立对话框窗口）
    pub fn set_custom_precision_dialog_open(&mut self, open: bool) {
        self.root.set_custom_precision_dialog_open(open);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 设置工程设置对话框是否打开（用于独立对话框窗口）
    pub fn set_project_settings_dialog_open(&mut self, open: bool) {
        self.root.set_project_settings_dialog_open(open);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 设置设置对话框是否打开（用于独立对话框窗口）
    pub fn set_settings_dialog_open(&mut self, open: bool) {
        self.root.set_settings_dialog_open(open);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 设置音符变速对话框是否打开（用于独立对话框窗口）
    pub fn set_speed_change_dialog_open(&mut self, open: bool) {
        self.root.set_speed_change_dialog_open(open);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 设置批量编辑对话框是否打开（用于独立对话框窗口）
    pub fn set_batch_edit_dialog_open(&mut self, open: bool) {
        self.root.set_batch_edit_dialog_open(open);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 设置导出进度对话框是否打开（用于独立对话框窗口）
    pub fn set_export_progress_dialog_open(&mut self, open: bool) {
        self.root.set_export_progress_dialog_open(open);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 设置内存监控对话框是否打开（用于独立对话框窗口）
    pub fn set_memory_monitor_dialog_open(&mut self, open: bool) {
        self.root.set_memory_monitor_dialog_open(open);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 设置找回删除音轨对话框是否打开（用于独立对话框窗口）
    pub fn set_recover_track_dialog_open(&mut self, open: bool) {
        self.root.set_recover_track_dialog_open(open);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 设置找回删除音轨对话框的条目列表（Runner 扫描缓存目录后调用）
    pub fn set_recover_track_dialog_entries(
        &mut self,
        entries: Vec<crate::state::root_state::RecoverTrackEntry>,
    ) {
        self.root.set_recover_track_dialog_entries(entries);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// Runner 扫描 `.lmdeltrack` 缓存目录后，通过此方法把条目列表回填给 UI
    ///
    /// 内部把 `RecoverTrackEntryPayload` 转换为 UI 状态结构并填充对话框。
    pub fn apply_recover_track_entries(
        &mut self,
        entries: Vec<lumino_message::events::window::track::RecoverTrackEntryPayload>,
    ) {
        self.root.apply_recover_track_entries(entries);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// Runner 加载 `.lmdeltrack` 后，通过此方法把音轨重新加入 sidebar.tracks + editor_state
    pub fn apply_track_restored(
        &mut self,
        payload: lumino_message::events::window::track::TrackDeletionPayload,
    ) {
        self.root.apply_track_restored(payload);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// Runner 永久销毁 `.lmdeltrack` 后，通过此方法释放 reserved_track_id
    pub fn apply_track_permanently_deleted(&mut self, track_id: u16) {
        self.root.apply_track_permanently_deleted(track_id);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 应用音符变速到主窗口
    pub fn apply_speed_change(&mut self, factor: f32) {
        self.root.apply_speed_change(factor);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 应用批量编辑到主窗口
    pub fn apply_batch_edit(&mut self, velocity: &str, gate: &str, key: &str, tick: &str) {
        self.root.apply_batch_edit(velocity, gate, key, tick);
        // 强制使 note 渲染缓存失效：`mark_notes_changed` 设置的 `note_index_dirty`
        // 可能被 hit_test 路径的 `ensure_spatial_index()` 提前清除，导致下一帧
        // `prepare_notes_if_needed` 跳过实例重建，音符视觉位置不更新。
        self.render_ctx.render_cache.note_viewport_hash = 0;
        self.render_ctx.render_cache.note_render_viewport = None;
        self.root.editor.grid_cache.clear();
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 重置工程设置对话框状态到默认值（关闭工程 / 新建工程 / 加载新文件时调用）
    pub fn reset_project_settings(&mut self) {
        self.root.reset_project_settings();
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 设置工程设置对话框数据（用于独立对话框窗口）
    pub fn set_project_settings_data(&mut self, data: crate::root::ProjectSettingsDialogData) {
        self.root.set_project_settings_data(data);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 应用工程设置到主窗口
    pub fn apply_project_settings(
        &mut self,
        title: String,
        tempo: f64,
        copyright: String,
        author: String,
        time_signatures: Vec<(u32, u8, u8)>,
    ) {
        self.root
            .apply_project_settings(title, tempo, copyright, author, time_signatures);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 应用设置面板配置到主窗口
    pub fn apply_settings(&mut self, settings: crate::settings::SettingsPanel, theme: String) {
        // 同步主题
        if self.root.window.theme.to_string() != theme {
            tracing::info!("同步主题: {} -> {}", self.root.window.theme, theme);
            self.route_message(crate::window::Event::theme(theme));
            self.root.editor.grid_cache.clear();
            self.root.editor.keyboard_cache.clear();
            self.root.editor.ruler_cache.clear();
            self.render_ctx.render_cache.grid_viewport_hash = 0;
            self.render_ctx.render_cache.note_viewport_hash = 0;
            self.render_ctx.render_cache.note_render_viewport = None;
        }

        self.root.apply_settings(settings);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 获取当前项目设置数据（用于填充工程设置对话框）
    #[allow(clippy::type_complexity)]
    pub fn get_project_settings_data(
        &self,
    ) -> (
        String,
        String,
        String,
        String,
        String,
        f64,
        Vec<(u32, u8, u8)>,
    ) {
        self.root.get_project_settings_data()
    }

    /// 获取已保存的项目标题（不含"无标题"回退，用于对话框窗口标题）
    pub fn get_project_settings_title(&self) -> String {
        self.root.state.project_settings_dialog.title.clone()
    }

    /// 获取当前工程的作者（工程设置对话框填写，保存 LMPJ/素材时写入 metadata）
    pub fn get_project_author(&self) -> String {
        self.root.state.project_settings_dialog.author.clone()
    }

    /// 获取当前工程的版权信息（工程设置对话框填写）
    pub fn get_project_copyright(&self) -> String {
        self.root.state.project_settings_dialog.copyright.clone()
    }

    /// 获取并清空对话框结果
    pub fn take_dialog_result(&mut self) -> Option<DialogResult> {
        self.root.take_dialog_result()
    }

    /// 设置自定义精度值（用于独立对话框窗口）
    pub fn set_custom_precision(&mut self, ticks: f32) {
        self.root.set_custom_precision(ticks);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 设置协作对话框是否打开（用于独立对话框窗口）
    pub fn set_collaboration_dialog_open(&mut self, open: bool) {
        self.root.set_collaboration_dialog_open(open);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 设置协作视图状态（用于独立对话框窗口）
    ///
    /// 返回视图状态是否发生变更（用于 runner 判断是否需要广播）。
    pub fn set_collaboration_view_state(
        &mut self,
        state: CollaborationViewState,
        invite_code: Option<String>,
        room_name: Option<String>,
    ) -> bool {
        let changed = self
            .root
            .set_collaboration_view_state(state, invite_code, room_name);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
        changed
    }

    /// 更新远端鼠标位置
    pub fn update_remote_cursor(
        &mut self,
        user_id: String,
        x: f32,
        y: f32,
        color: String,
        username: String,
    ) {
        self.route_message(message::Message::Collaboration(
            lumino_message::CollaborationAction::RemoteMouseMoved {
                user_id: user_id.into(),
                x,
                y,
                color: color.into(),
                username: username.into(),
            },
        ));
        self.window_ctx.window.request_redraw();
    }

    /// 移除远端鼠标
    pub fn remove_remote_cursor(&mut self, user_id: String) {
        self.route_message(message::Message::Collaboration(
            lumino_message::CollaborationAction::RemoteUserLeft {
                user_id: user_id.into(),
            },
        ));
        self.window_ctx.window.request_redraw();
    }

    /// 应用远端用户的选择更新（高亮 + first-writer-wins 冲突判定）
    pub fn apply_remote_selection(&mut self, user_id: String, selection: String, color: String) {
        self.route_message(message::Message::Collaboration(
            lumino_message::CollaborationAction::RemoteSelection {
                user_id: user_id.into(),
                selection,
                color: color.into(),
            },
        ));
        self.window_ctx.window.request_redraw();
    }

    /// 更新远端音符
    pub fn update_remote_note(&mut self, operation: String) {
        self.route_message(message::Message::Collaboration(
            lumino_message::CollaborationAction::RemoteNoteUpdate { operation },
        ));
        self.window_ctx.window.request_redraw();
    }
    /// 应用远程笔记操作到本地编辑器（委托给 Root 实现）
    pub fn apply_remote_note_operation(
        &mut self,
        operation: &lumino_collaboration::types::NoteBatchOperation,
    ) {
        self.root.apply_remote_note_operation(operation);
        self.window_ctx.window.request_redraw();
    }

    /// 添加远程音轨（委托给 Root 实现）
    pub fn add_remote_track(&mut self, track_idx: usize) {
        self.root.add_remote_track(track_idx);
        self.window_ctx.window.request_redraw();
    }

    /// 获取当前 PPQ (Pulses Per Quarter note)
    pub fn ppq(&self) -> u16 {
        self.root.editor.editor_state.view.ppq
    }

    /// 更新进度
    pub fn update_progress(&mut self, progress: Option<(String, f64)>) {
        self.route_message(message::Message::Progress(progress));
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 更新导出进度对话框
    pub fn update_export_progress(&mut self, message: String, progress: f64) {
        self.root.update_export_progress(message, progress);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 标记导出渲染完成
    pub fn set_export_render_completed(&mut self) {
        self.root.state.audio_export_dialog.is_rendering = false;
        self.root.state.audio_export_dialog.render_completed = true;
        self.root.state.audio_export_dialog.render_progress = 1.0;
        self.root.state.audio_export_dialog.render_message = "导出完成".to_string();
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 标记导出渲染失败
    pub fn set_export_render_failed(&mut self, error: String) {
        self.root.state.audio_export_dialog.is_rendering = false;
        self.root.state.audio_export_dialog.render_error = Some(error.clone());
        self.root.state.audio_export_dialog.render_message = format!("导出失败: {error}");
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 更新主题
    pub fn update_theme(&mut self, theme: String) {
        self.route_message(window::Event::theme(theme));
        self.root.editor.grid_cache.clear();
        self.root.editor.keyboard_cache.clear();
        self.root.editor.ruler_cache.clear();
        self.render_ctx.render_cache.grid_viewport_hash = 0;
        self.render_ctx.render_cache.note_viewport_hash = 0;
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 打开协作对话框并设置为连接中状态（用于调试模式自动连接）
    pub fn open_collaboration_dialog_with_state(
        &mut self,
        host: String,
        port: u16,
        username: String,
    ) {
        self.root
            .open_collaboration_dialog_with_state(host, port, username);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 从另一个 Host 同步协作状态（用于对话框窗口同步主窗口状态）
    pub fn sync_collaboration_state_from(&mut self, other: &Host) {
        self.root.sync_collaboration_state_from(&other.root);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 从另一个 Host 同步云存储 UI 状态（对话框窗口同步主窗口快照）。
    ///
    /// 云存储唯一数据源是主窗口 Root：连接快照/目录条目/提示信息均注入主窗口，
    /// 设置面板云管理页与云文件浏览器通过本方法获取最新状态。
    pub fn sync_cloud_state_from(&mut self, other: &Host) {
        self.root.sync_cloud_state_from(&other.root);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 从另一个 Host 同步云存储**共享快照**（运行期广播用）。
    ///
    /// 只同步共享/浏览数据，**排除连接表单字段**，避免用户在连接面板
    /// 输入时被后台状态广播覆盖。
    pub fn sync_cloud_snapshot_from(&mut self, other: &Host) {
        self.root.sync_cloud_snapshot_from(&other.root);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }
}
