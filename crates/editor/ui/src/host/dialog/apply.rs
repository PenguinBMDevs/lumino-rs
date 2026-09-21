//! Host 对话框应用子模块 — apply_* 数据回写与应用

use crate::host::Host;
use crate::window;

impl Host {
    /// 应用画刷「绘制行为」配置到主窗口（对话框 Save 后由 runner 调用）
    pub fn apply_brush_settings(&mut self, config: lumino_core::BrushConfig) {
        self.root.toolbar.brush = config.clone();
        self.root.editor.brush = config;
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

    /// 回填从已加载 `.lmpj` 工程文件恢复的作者与版权（关闭工程后重开面板显示正确值）
    pub fn set_project_author_and_copyright(&mut self, author: String, copyright: String) {
        self.root.apply_loaded_project_metadata(author, copyright);
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
