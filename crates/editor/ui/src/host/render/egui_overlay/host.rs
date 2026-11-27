//! `Host` 侧的叠加层接线：渲染入口、指针归属与 IME 应用。
//!
//! 指针归属 = 「面板占用区 ∪ 非 Background 的 egui 图层」；判定只查询
//! `EguiOverlay::pointer_blocked` 与 `ctx.layer_id_at`，不维护第二份清单。

use super::*;

impl Host {
    /// 绘制 egui 外壳。
    ///
    /// `clear = Some(color)` 时 egui 独占整窗（生产渲染路径）；`None` 由调用方
    /// 保证底层已绘制（当前生产路径不使用）。
    pub(crate) fn render_egui_overlay(
        &mut self,
        frame: &wgpu::SurfaceTexture,
        view: &wgpu::TextureView,
        clear: Option<wgpu::Color>,
        egui_events: Vec<egui::Event>,
        egui_focused: bool,
    ) {
        let format = frame.texture.format();
        let size = self.render_ctx.viewport.physical_size();
        let pixels_per_point = self.render_ctx.viewport.scale_factor();
        let app_frame = self.frame_count;
        // 工具栏折叠（溢出菜单）依赖实际可用宽度：与生产 `responsive` 同口径。
        let logical_width = size.width as f32 / pixels_per_point;
        let logical_height = size.height as f32 / pixels_per_point;
        let model = self.root.egui_chrome_model_for(logical_width);
        // 主内容区（标题栏下、状态栏上）：渲染器入口面板 / 时间轴 / 预览共用一份几何。
        let window_area = egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(logical_width, logical_height),
        );
        // 画布几何：egui 是唯一布局来源（上一代 不再参与，取代 `Grid` program
        // 的 `CanvasBoundsChanged`）。
        self.sync_canvas_bounds(&model, window_area);
        let content_area = {
            let left = model
                .sidebar
                .as_ref()
                .map(|sidebar| sidebar.total_width())
                .unwrap_or(48.0);
            let top = if model.titlebar.is_some() {
                lumino_ui_egui::layout::TITLEBAR_H
            } else {
                0.0
            };
            egui::Rect::from_min_max(
                egui::pos2(left, top),
                egui::pos2(
                    logical_width,
                    (logical_height - lumino_ui_egui::layout::STATUSBAR_H).max(top),
                ),
            )
        };
        // 形状抽屉：面板 + 画布都由 egui 绘制（上一代 只保留标题栏）。
        let shape_panel_rect = (self.root.state.dialog_type
            == crate::state::root_state::DialogType::ShapeDrawer)
            .then(|| {
                let width = self.root.state.shape_drawer.panel_width;
                // 对话框窗口：`ChromeModel.titlebar` 为 None，但 egui 仍画 30px
                // 自制标题栏（`chrome::popup_titlebar`），因此这里按 `use_native_titlebar`
                // 判断（与画布动作端的 rect 同源）。
                let top = if self.root.settings.synth.use_native_titlebar {
                    0.0
                } else {
                    lumino_ui_egui::layout::TITLEBAR_H
                };
                egui::Rect::from_min_max(egui::pos2(0.0, top), egui::pos2(width, logical_height))
            });
        let shape_drawer = shape_panel_rect.map(|rect| self.root.shape_drawer_model(rect));
        let shape_canvas = shape_panel_rect.map(|panel_rect| {
            let rect = egui::Rect::from_min_max(
                egui::pos2(panel_rect.right() + SPLITTER_W, panel_rect.top()),
                egui::pos2(logical_width, panel_rect.bottom()),
            );
            self.root
                .shape_canvas_model(rect, &self.root.egui_palette())
        });
        // 视频导出面板：整块替换右侧内容区（与音频导出同形态）。
        let video_export = (self.root.sidebar.video_export_visible
            && !self.root.is_arrangement_mode())
        .then(|| {
            let left = model
                .sidebar
                .as_ref()
                .map(|sidebar| sidebar.total_width())
                .unwrap_or(48.0);
            let top = if self.root.settings.synth.use_native_titlebar {
                0.0
            } else {
                lumino_ui_egui::layout::TITLEBAR_H
            };
            self.root.video_export_panel_model(egui::Rect::from_min_max(
                egui::pos2(left, top),
                egui::pos2(logical_width, logical_height),
            ))
        });
        // 音频导出面板：整块替换右侧内容区（工具栏 / 画布 / 状态栏）。
        let audio_export = (self.root.sidebar.audio_export_visible
            && !self.root.is_arrangement_mode())
        .then(|| {
            let left = model
                .sidebar
                .as_ref()
                .map(|sidebar| sidebar.total_width())
                .unwrap_or(48.0);
            let top = if model.titlebar.is_some() {
                lumino_ui_egui::layout::TITLEBAR_H
            } else {
                0.0
            };
            lumino_ui_egui::chrome::AudioExportPanelModel {
                rect: egui::Rect::from_min_max(
                    egui::pos2(left, top),
                    egui::pos2(logical_width, logical_height),
                ),
                content: self.root.audio_export_model(),
            }
        });
        let palette = self.root.egui_palette();
        // 瀑布流面板可见且纹理已渲染时，把视图交给 egui 采样（上一代 侧同位置
        // 只保留占位，不会二次采样同一张纹理）。
        let waterfall = {
            let sidebar = &self.root.right_sidebar;
            (sidebar.panel_visible
                && sidebar.active_panel == crate::right_sidebar::RightSidebarPanel::PianoWaterfall
                && self.root.right_sidebar_visible())
            .then(|| sidebar.piano_waterfall.waterfall_view.clone())
            .flatten()
        };
        // 取走本帧累积的指针 / 滚轮 / 修饰键 / 焦点事件。
        let events = egui_events;
        let focused = egui_focused;
        // Device/Queue 是 Arc 句柄，克隆避免与 `egui_overlay` 的可变借用冲突。
        let device = self.render_ctx.device.clone();
        let queue = self.render_ctx.queue.clone();

        let overlay = self
            .render_ctx
            .egui_overlay
            .get_or_insert_with(|| EguiOverlay::new(&device, format));
        let overlay_output = overlay.draw(OverlayFrame {
            device: &device,
            queue: &queue,
            view,
            format,
            size_px: [size.width, size.height],
            pixels_per_point,
            app_frame,
            model: &model,
            palette,
            waterfall,
            audio_export,
            shape_drawer,
            shape_canvas,
            video_export,
            renderer_panel: self.root.renderer_panel_model(content_area),
            arrangement: self.root.arrangement_model(),
            video_timeline: self.root.video_timeline_model(content_area),
            // `MidiLoadLock::is_loaded` 是「真实 MIDI/工程已载入」的唯一权威源：
            // 启动 / 新建 / 关闭工程后的空白工程为 false（显示欢迎屏分栏），
            // 真实载入文件为 true（只显示品牌占位）。禁止在这里取反。
            first_screen: self.root.first_screen_model(self.midi_load.is_loaded()),
            video_preview: self.root.video_preview_model(window_area, content_area),
            mask_panel: self.root.mask_panel_model(),
            toast: self.root.egui_toast_model(),
            help_tip: self.root.egui_help_tip_model(),
            egg: self.root.egui_egg_model(),
            piano_roll_menu: self.root.piano_roll_context_menu_model(),
            editor_buttons: self.root.editor_buttons_model(),
            marquee: self.root.marquee_model(),
            shape_edit: self.root.shape_edit_model(),
            shape_list: self.root.shape_list_model(),
            area_palette: self.root.area_palette_model(),
            shape_preview: self.root.shape_preview_model(),
            curve_preview: self.root.curve_preview_model(),
            remote_cursors: self.root.remote_cursors_model(),
            editor_overlay: self.root.editor_overlay_model(),
            i2m_region: self.root.i2m_region_model(),
            bend_buttons: self.root.bend_buttons_model(),
            text_tool: self.root.text_tool_model(),
            velocity_panel: self.root.velocity_panel_model(),
            render_preview: self.root.waterfall_player.view.clone(),
            editor_texture: self.egui_editor_texture.clone(),
            events,
            focused,
            clear,
        });
        // 各通道动作统一取走并分发（`actions.rs`）。
        let actions = actions::OverlayActions::take(&overlay.ctx);
        // egui 的弹层（ComboBox / Tooltip）首帧是「测量 pass」，不产生绘制图元：
        // 它会在本帧请求一次重绘，必须立刻再排一帧，否则下拉菜单要等到下一次
        // 鼠标事件才出现。
        if overlay.ctx.has_requested_repaint() {
            self.window_ctx.window.request_redraw();
        }
        // 占用区已由 `EguiOverlay` 在本帧绘制时保存，`Host` 不再持有副本。
        self.apply_egui_cursor(overlay_output.cursor);
        self.apply_egui_ime(overlay_output.ime, pixels_per_point);
        self.apply_overlay_actions(actions, logical_width);
    }

    /// 当前指针是否落在 egui 占用区（菜单 / 弹层 / 面板 / 模态）。
    pub(crate) fn egui_pointer_blocked(&self) -> bool {
        let Some(pos) = self.cursor_egui() else {
            return false;
        };
        self.render_ctx
            .egui_overlay
            .as_ref()
            .is_some_and(|overlay| overlay.pointer_blocked(pos))
    }

    /// 宿主记录的逻辑光标 → egui 坐标。
    ///
    /// 输入缓冲已归运行时；`Host` 只读 `window_ctx.cursor_position`，触摸路径也
    /// 会写同一个字段，因此主窗口与对话框共用这一份判定。
    pub(crate) fn cursor_egui(&self) -> Option<egui::Pos2> {
        self.window_ctx
            .cursor_position
            .map(|p| egui::pos2(p.x, p.y))
    }

    /// 指定点是否被 egui 的**浮层图层**（菜单 / 弹层 / 面板 / 模态）占用。
    ///
    /// 与 [`Self::egui_pointer_blocked`] 的区别：后者是手工登记的 root 层矩形，
    /// 只能覆盖"记得登记"的 band；这里直接问 egui 的图层栈——凡是 `Area` /
    /// `Window` / ComboBox 弹层，命中点都会返回非 `Background` 图层。
    /// 这消除了「弹层没登记 → 点击同时落到画布」的一整类漏点击。
    pub(crate) fn egui_layer_owns_pos(&self, pos: egui::Pos2) -> bool {
        let Some(overlay) = self.render_ctx.egui_overlay.as_ref() else {
            return false;
        };
        overlay
            .ctx
            .layer_id_at(pos)
            .is_some_and(|layer| layer.order != egui::Order::Background)
    }

    /// egui 本帧是否正在编辑文本（下一帧键盘事件只给它）。
    pub fn egui_wants_keyboard(&self) -> bool {
        self.render_ctx
            .egui_overlay
            .as_ref()
            .is_some_and(|overlay| overlay.ctx.text_edit_focused())
    }

    /// 把 egui 的 IME 锚点应用到系统窗口。
    ///
    /// - `Some`：文本框正在编辑 → 开启输入法，并把候选窗锚到光标矩形；
    /// - `None`：编辑结束 → 关闭输入法。
    ///
    /// `set_ime_allowed` 只在状态变化时调用，避免每帧重复设置。
    fn apply_egui_ime(&mut self, ime: Option<egui::output::IMEOutput>, pixels_per_point: f32) {
        let window = std::sync::Arc::clone(&self.window_ctx.window);
        apply_ime_to_window(
            &*window,
            &mut self.window_ctx.ime_allowed,
            ime,
            pixels_per_point,
        );
    }

    /// 画布输入是否应让位给 egui（面板占用区 ∪ 浮层图层）。
    ///
    /// 指针取宿主记录的逻辑光标（触摸路径同样写入），不再依赖 egui 事件缓冲。
    pub(crate) fn egui_owns_pointer(&self) -> bool {
        let Some(pos) = self.cursor_egui() else {
            return false;
        };
        let (panel_blocked, layer_id) = self
            .render_ctx
            .egui_overlay
            .as_ref()
            .map_or((false, None), |overlay| {
                (overlay.pointer_blocked(pos), overlay.ctx.layer_id_at(pos))
            });
        let layer = layer_id.is_some_and(|id| id.order != egui::Order::Background);
        if pointer_diag() {
            let blocked_rects = self
                .render_ctx
                .egui_overlay
                .as_ref()
                .map_or(0, |overlay| overlay.blocked.len());
            tracing::info!(
                target: "lumino::pointer_diag",
                pos = ?pos,
                panel_blocked,
                layer,
                layer_id = ?layer_id,
                blocked_rects,
                "指针归属判定"
            );
        }
        panel_blocked || layer
    }
}

/// 指针归属诊断开关（`LUMINO_POINTER_DIAG=1`）。
///
/// 只用于真机取证：打开后每次鼠标按键打一条「指针归属判定」日志，
/// 用来区分「命中矩形漏登记」与「图层判定失效」两种穿透。
pub(super) fn pointer_diag() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("LUMINO_POINTER_DIAG").is_some())
}
