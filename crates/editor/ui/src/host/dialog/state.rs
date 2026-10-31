//! Host 对话框状态管理子模块 — 开关/标记类 setter 与状态查询

use lumino_core::shape::{ShapeDrawing, ShapeLibrary, ShapeRect};
use lumino_core::{tr, trf};
use lumino_note_core::history::OpKind;

use crate::host::{Host, ShapeInsertOptions, types::DialogResult};
use crate::message;
use crate::shape_drawer::fit::fit_contain_rect;
use crate::state::root_state::{CollaborationViewState, HistoryKind};

fn history_kind(kind: OpKind) -> HistoryKind {
    match kind {
        OpKind::NoteCreate => HistoryKind::NoteCreate,
        OpKind::NoteMove => HistoryKind::NoteMove,
        OpKind::NoteDelete => HistoryKind::NoteDelete,
        OpKind::NoteTransform => HistoryKind::NoteTransform,
        OpKind::VelocityEdit => HistoryKind::VelocityEdit,
        OpKind::AutomationEdit => HistoryKind::AutomationEdit,
        OpKind::Recording => HistoryKind::Recording,
        OpKind::Other | OpKind::ChainMarker => HistoryKind::Other,
    }
}

impl Host {
    /// 设置加载确认对话框（用于独立对话框窗口）
    pub fn set_load_confirm_dialog(&mut self, file_path: &str, size_mb: f64) {
        self.root.set_load_confirm_dialog(file_path, size_mb);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 查询当前工程是否存在未保存的更改
    ///
    /// 供关闭工程 / 打开另一个工程 / 退出软件前判断是否需要弹出
    /// 「是否保留未保存的更改」确认对话框。
    pub fn is_project_modified(&self) -> bool {
        self.root.editor.editor_state.data.modified
    }

    /// 标记当前工程为「已保存」状态（清零未保存更改标记）
    ///
    /// 保存完成 / 加载新文件 / 新建 / 关闭工程后调用，避免误弹保存确认对话框。
    pub fn mark_project_clean(&mut self) {
        self.root.editor.editor_state.data.modified = false;
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 打开保存确认对话框（用于独立对话框窗口）
    ///
    /// 关闭工程 / 打开另一个工程 / 退出软件前，若工程存在未保存更改时弹出。
    pub fn set_save_confirm_dialog(&mut self) {
        self.root.set_save_confirm_dialog_open(true);
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

    /// 设置「音符画设置」对话框是否打开（用于独立对话框窗口）
    pub fn set_draw_settings_dialog_open(&mut self, open: bool) {
        self.root.set_draw_settings_dialog_open(open);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 设置 Help 对话框是否打开（用于独立对话框窗口）
    pub fn set_help_dialog_open(&mut self, open: bool) {
        self.root.set_help_dialog_open(open);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 设置形状抽屉是否打开（用于独立窗口；DRAW-033）
    pub fn set_shape_drawer_dialog_open(&mut self, open: bool) {
        self.root.set_shape_drawer_dialog_open(open);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 设置撤销历史窗口是否打开（独立 OS 窗口；DRAW-035）
    pub fn set_history_dialog_open(&mut self, open: bool) {
        self.root.set_history_dialog_open(open);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 历史窗口打开期间先提交待落盘的批量移动/复制，再读取历史。
    pub fn flush_history_pending(&mut self) {
        self.root.editor.flush_pending_history();
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 读取主编辑器历史视图，投影为 UI 中性类型。
    ///
    /// 返回 `(steps, current)`；当前游标 0 表示初始状态。
    pub fn history_view(&self) -> (Vec<crate::state::root_state::HistoryStepView>, usize) {
        let view = self.root.editor.history_view();
        let steps = view
            .steps
            .into_iter()
            .map(|step| crate::state::root_state::HistoryStepView {
                index: step.index,
                kind: history_kind(step.kind),
            })
            .collect();
        (steps, view.current)
    }

    /// 注入历史面板数据；返回是否有变化。
    pub fn set_history_view(
        &mut self,
        steps: Vec<crate::state::root_state::HistoryStepView>,
        current: usize,
    ) -> bool {
        if !self.root.set_history_view(steps, current) {
            return false;
        }
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
        true
    }

    /// 从历史面板跳到指定游标；形状 group / 协作广播由 Editor 统一处理。
    pub fn history_jump_to(&mut self, target: usize) -> bool {
        let ok = self.root.editor.history_jump_to(target);
        if ok {
            self.ui_dirty = true;
            self.window_ctx.window.request_redraw();
        }
        ok
    }

    /// 注入形状库目录（Runner 在抽屉 UI 就绪后调用）
    pub fn set_shape_drawer_dir(&mut self, dir: std::path::PathBuf) {
        let (library, warnings) = ShapeLibrary::load_dir(&dir);
        let state = &mut self.root.state.shape_drawer;
        state.shapes_dir = dir;
        state.library = library;
        state.warnings = warnings;
        state.status = None;
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 把抽屉当前图形插入主工程（当前轨道 + 播放头；尺寸/比例来自抽屉插入参数）。
    pub fn insert_shape_drawing(
        &mut self,
        drawing: &ShapeDrawing,
        options: ShapeInsertOptions,
    ) -> Result<(), String> {
        let track = self.root.editor.editor_state.data.current_track;
        if track == 0 {
            return Err("指挥轨不能插入形状，请先选择普通音轨".to_string());
        }
        if !options.beats.is_finite() || options.beats <= 0.0 {
            return Err("插入宽度需为正数（拍）".to_string());
        }
        if !options.keys.is_finite() || !(1.0..=128.0).contains(&options.keys) {
            return Err("插入高度需在 1–128 key".to_string());
        }
        let ppq = f32::from(self.root.editor.editor_state.view.ppq);
        let tick = self.root.editor.playback_position.max(0.0);
        let key_lo = 60.0;
        let target = ShapeRect::normalized(
            tick,
            key_lo,
            tick + options.beats * ppq,
            key_lo + options.keys,
        );
        let rect = if options.fit_contain {
            let source = drawing.bounds().unwrap_or(target);
            fit_contain_rect(source, target, ppq)
        } else {
            target
        };
        let model = drawing
            .to_model(track as u16, 1.0, rect)
            .map_err(|error| error.to_string())?;
        if !self.root.editor.create_shape(model) {
            return Err("形状插入失败（请检查当前音轨）".to_string());
        }
        self.root.state.shape_drawer.status = Some(format!(
            "已插入当前工程（{} 拍 × {} key{}）",
            options.beats,
            options.keys,
            if options.fit_contain {
                "，保持比例"
            } else {
                ""
            }
        ));
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
        Ok(())
    }

    /// 种入画刷「绘制行为」对话框本地草稿（Runner 在对话框 UI 就绪后注入）
    pub fn set_brush_settings_draft(&mut self, config: lumino_core::BrushConfig) {
        self.root.state.brush_settings_draft = config;
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 收集可选音轨列表（仅普通音轨，排除指挥轨），供画刷「绘制行为」对话框使用
    ///
    /// 返回 `(track_id, name)` 列表，顺序与侧边栏一致。
    pub fn normal_track_choices(&self) -> Vec<(usize, String)> {
        self.root
            .sidebar
            .tracks
            .iter()
            .filter(|t| !t.is_conductor)
            .map(|t| (t.id, t.name.clone()))
            .collect()
    }

    /// 种入画刷「绘制行为」对话框可选音轨列表（仅普通音轨，排除指挥轨）
    pub fn set_brush_settings_tracks(&mut self, tracks: Vec<(usize, String)>) {
        self.root.state.brush_settings_tracks = tracks;
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

    /// 设置工程设置对话框数据（用于独立对话框窗口）
    pub fn set_project_settings_data(&mut self, data: crate::root::ProjectSettingsDialogData) {
        self.root.set_project_settings_data(data);
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 轻量更新工程设置对话框的音符总量（UI-015；返回是否有变化）。
    ///
    /// 值变化才置脏并请求重绘，供 Runner 在对话框打开期间每帧同步使用。
    pub fn set_project_settings_note_count(&mut self, count: usize) -> bool {
        if !self.root.set_project_settings_note_count(count) {
            return false;
        }
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
        true
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
        self.root.state.audio_export_dialog.is_paused = false;
        self.root.state.audio_export_dialog.render_completed = true;
        self.root.state.audio_export_dialog.render_progress = 1.0;
        self.root.state.audio_export_dialog.render_message = tr!("导出完成").to_string();
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }

    /// 标记导出渲染失败
    pub fn set_export_render_failed(&mut self, error: String) {
        self.root.state.audio_export_dialog.is_rendering = false;
        self.root.state.audio_export_dialog.is_paused = false;
        // “已中止”为预期操作，不加“导出失败”前缀
        if error == "已中止" {
            self.root.state.audio_export_dialog.render_error = Some(error.clone());
            self.root.state.audio_export_dialog.render_message = tr!("已中止").to_string();
        } else {
            self.root.state.audio_export_dialog.render_error = Some(error.clone());
            self.root.state.audio_export_dialog.render_message =
                trf!("导出失败: {error}", error = error);
        }
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
    }
}
