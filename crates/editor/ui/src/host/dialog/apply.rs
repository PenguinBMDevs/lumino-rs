//! Host 对话框应用子模块 — apply_* 数据回写与应用

use crate::host::Host;
use crate::window;

mod onion;

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
    pub fn set_project_author_and_copyright(
        &mut self,
        author: String,
        copyright: String,
        use10: bool,
        i2m: Option<lumino_core::I2mSettings>,
    ) {
        self.root
            .apply_loaded_project_metadata(author, copyright, use10, i2m);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 回填从已加载 `.lmpj` 工程恢复的端口音色库映射（REND-005）。
    ///
    /// 与作者/版权同一条加载链路：`reset_project_settings` 清空后调用本方法。
    pub fn apply_project_port_soundfonts(
        &mut self,
        map: lumino_midi_model::multi_port::PortSoundfontMap,
    ) {
        self.root.set_port_soundfonts(map);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 注入端口音色库对话框数据并打开（独立窗口；REND-005）。
    pub fn set_port_soundfonts_dialog(
        &mut self,
        map: lumino_midi_model::multi_port::PortSoundfontMap,
        document_max_port: u8,
    ) {
        self.root
            .set_port_soundfonts_dialog(&map, document_max_port);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 当前工程级图片转 MIDI 参数（保存 `.lmpj` / 云保存时写入 metadata）
    pub fn get_project_i2m_config(&self) -> lumino_core::I2mSettings {
        self.root.right_sidebar.config.to_settings()
    }

    /// 应用工程设置到主窗口
    pub fn apply_project_settings(
        &mut self,
        title: String,
        tempo: f64,
        copyright: String,
        author: String,
        time_signatures: Vec<(u32, u8, u8)>,
        use10: bool,
    ) {
        self.root
            .apply_project_settings(title, tempo, copyright, author, time_signatures, use10);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 应用设置面板配置到主窗口
    pub fn apply_settings(&mut self, mut settings: crate::settings::SettingsPanel, theme: String) {
        // 同步主题：与设置面板预览、View 菜单共用同一条主题事务，避免
        // "清缓存/置脏/请求重绘"三处各写一半导致网格与 UI 不同步。
        if self.root.window.theme.to_string() != theme {
            self.apply_theme_change(window::Event::theme(theme));
        }

        // 洋葱皮渲染后端：锁定/模式不兼容时拒绝并把值回写成运行时真值，
        // 必须早于 `root.apply_settings`（否则主 Root 会记下一个没生效的后端）。
        self.apply_onion_backend_switch(&mut settings);

        self.root.apply_settings(settings);
        // UI-001（egui 版）：字体改动立即生效——登记新字体后代数变化，
        // 各 egui Context 在下一帧 `fonts::install` 时重建字体表，无需重启。
        crate::font::apply(
            &self.root.settings.editing.program_font_name,
            &self.root.settings.editing.program_font_path,
        );
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 更新主题（对外入口，供 Runner / 对话框广播调用）
    pub fn update_theme(&mut self, theme: String) {
        self.apply_theme_change(window::Event::theme(theme));
    }

    /// 主题变更的**唯一事务入口**（主窗口 / 对话框 / 设置面板预览 / View 菜单共用）。
    ///
    /// 一次完成四件事，少任何一步都会出现"UI 已换主题、网格还是上一主题"：
    /// 1. 路由 `Theme` 事件 → `Root::window.theme` 换新（egui 外壳立即生效）；
    /// 2. 失效 UI 侧绘制缓存（`grid_cache` / `keyboard_cache` / `ruler_cache`）；
    /// 3. **标记离屏内容脏**：主题色是离屏帧的 uniform 输入，属于内容变化。
    ///    不置脏 ⇒ `redraw_separate_thread` 走「允许 present 落后一帧」分支，且
    ///    视口未变不会登记 `pending_present_flush` ⇒ 屏幕长期停在上一主题的
    ///    离屏帧，直到下一次无关重绘才被替换（2026-10-28 网格背景慢一步的根因）；
    /// 4. 置 `ui_dirty` 并请求重绘，确保新帧一定会被生产出来。
    ///
    /// 四件事必须原子完成：拆到各入口各写一半，就是"抽屉换了、网格没换"的来源。
    pub(crate) fn apply_theme_change(&mut self, message: crate::message::Message) {
        let old = self.root.window.theme.to_string();
        self.route_message(message);
        self.invalidate_theme_caches();
        tracing::info!(
            old_theme = %old,
            new_theme = %self.root.window.theme,
            "主题已切换：离屏内容标记为脏（有渲染线程的窗口本帧同步呈现新主题）"
        );
    }

    /// 主题相关的缓存失效 + 内容脏标记（供 [`Self::apply_theme_change`] 使用）。
    fn invalidate_theme_caches(&mut self) {
        // 关键：主题 = 离屏内容，不是"视口变化"。纯视口变化可以落后一帧，
        // 主题变化落后一帧就表现为「网格慢一步、移动鼠标才恢复」。
        self.render_ctx.mark_render_content_dirty();
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

    /// 从主窗口同步 MIDI 设备状态（设置对话框独立 Root 的输入 API 注入）。
    ///
    /// 设置窗没有自己的 `MidiManager`：不注入输入 API 就无法枚举设备，页面
    /// 永远显示「无可用设备」；不复制已扫描列表则主窗刚录过音的设备选择也会丢。
    /// 只同步设备相关字段，不覆盖对话框内其它正在编辑的设置。
    pub fn sync_midi_state_from(&mut self, other: &Host) {
        self.root.midi.api = other.root.midi.api.clone();
        self.root.settings.midi.devices = other.root.settings.midi.devices.clone();
        self.root.settings.midi.selected_device = other.root.settings.midi.selected_device;
        self.root.settings.midi.winmm_outputs = other.root.settings.midi.winmm_outputs.clone();
        self.root.settings.midi.selected_winmm_output =
            other.root.settings.midi.selected_winmm_output;
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }
}
