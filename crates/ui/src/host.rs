use std::{sync::Arc, time::Instant};

use iced_wgpu::{
    Engine, Renderer,
    graphics::{Shell, Viewport},
    wgpu,
};
use lumino_gfx::NoteRenderer;

use iced_winit::{
    Clipboard, conversion,
    runtime::user_interface::{self, UserInterface},
    winit,
};

use iced_core::{Event, Font, Pixels, Size, mouse, renderer, touch};

use crate::{config, root, window, message, toolbar, settings};

/// UI 宿主 - 管理 iced 渲染和 wgpu 音符渲染
pub struct Host {
    window: Arc<winit::window::Window>,
    root: root::Root,
    renderer: Renderer,
    events: Vec<Event>,
    cursor: mouse::Cursor,
    cache: user_interface::Cache,
    clipboard: Clipboard,
    viewport: Viewport,
    pending_window_action: Option<window::TrafficAction>,
    pending_drag: bool,
    /// 逻辑光标位置（用于音符预览）
    cursor_position: Option<iced_core::Point>,
    last_fps_update: Instant,
    /// 帧计数器（用于 FPS 计算）
    frame_count: u32,
    /// 是否正在拖拽调整工具栏高度
    is_toolbar_resizing: bool,
    /// 音符渲染器
    note_renderer: NoteRenderer,
    /// 上一帧时间
    last_frame_time: Instant,
}

impl Host {
    pub fn new(
        window: Arc<winit::window::Window>,
        width: u32,
        height: u32,
        ui_config: &config::UiConfig,
        gfx: &lumino_gfx::Context,
        is_progress: bool,
    ) -> Self {
        let viewport =
            Viewport::with_physical_size(Size::new(width, height), window.scale_factor() as f32);

        let clipboard = Clipboard::connect(window.clone());

        // 初始化 iced 渲染器
        let renderer = {
            let engine = Engine::new(
                &gfx.adapter,
                gfx.device.clone(),
                gfx.queue.clone(),
                gfx.format,
                None,
                Shell::headless(),
            );
            Renderer::new(engine, Font::default(), Pixels::from(16))
        };

        // 创建 wgpu 音符渲染器
        let note_renderer = NoteRenderer::new(&gfx.device, gfx.format);

        Self {
            window,
            root: if is_progress {
                root::Root::new_progress(&ui_config.theme)
            } else {
                root::Root::new(ui_config)
            },
            renderer,
            events: Vec::new(),
            cursor: mouse::Cursor::Unavailable,
            cache: user_interface::Cache::new(),
            clipboard,
            viewport,
            pending_window_action: None,
            pending_drag: false,
            note_renderer,
            cursor_position: None,
            last_frame_time: Instant::now(),
            last_fps_update: Instant::now(),
            frame_count: 0,
            is_toolbar_resizing: false,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.viewport = Viewport::with_physical_size(
            Size::new(width, height),
            self.window.scale_factor() as f32,
        );
    }

    pub fn redraw_requested(
        &mut self,
        frame: &wgpu::SurfaceTexture,
        view: &wgpu::TextureView,
        gfx: &lumino_gfx::Context,
    ) {
        // 计算 FPS
        self.frame_count += 1;
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_fps_update);

        if elapsed.as_millis() >= 50 {
            let fps = self.frame_count as f32 / elapsed.as_secs_f32();
            self.root.update(window::Event::fps_update(fps));
            self.frame_count = 0;
            self.last_fps_update = now;
        }

        self.last_frame_time = now;

        // 第一步：使用 wgpu 渲染音符（位于 UI 层下方）
        self.render_notes(frame, view, gfx);

        // 第二步：渲染 iced UI
        self.render_iced_ui(frame, view);
    }

    /// 渲染 iced UI 层
    fn render_iced_ui(&mut self, frame: &wgpu::SurfaceTexture, texture_view: &wgpu::TextureView) {
        // 临时取出缓存以避免借用冲突
        let cache = std::mem::take(&mut self.cache);

        // 构建视图和界面
        let root_view = self.root.view();
        let mut interface = UserInterface::build(
            root_view,
            self.viewport.logical_size(),
            cache,
            &mut self.renderer,
        );

        let mut messages = Vec::new();
        let (state, _) = interface.update(
            &[Event::Window(iced_core::window::Event::RedrawRequested(
                Instant::now(),
            ))],
            self.cursor,
            &mut self.renderer,
            &mut self.clipboard,
            &mut messages,
        );

        // 绘制界面
        let theme = self.root.theme();
        interface.draw(
            &mut self.renderer,
            &theme,
            &renderer::Style::default(),
            self.cursor,
        );

        // 归还缓存
        self.cache = interface.into_cache();

        self.renderer
            .present(None, frame.texture.format(), texture_view, &self.viewport);

        // 处理消息（在 interface 被释放之后）
        for message in messages {
            self.root.update(message);
        }

        // 更新鼠标光标
        if let user_interface::State::Updated {
            mouse_interaction, ..
        } = state
        {
            if let Some(icon) = iced_winit::conversion::mouse_interaction(mouse_interaction) {
                self.window.set_cursor(icon);
                self.window.set_cursor_visible(true);
            } else {
                self.window.set_cursor_visible(false);
            }
        }
    }

    /// 使用 wgpu 渲染音符
    fn render_notes(
        &mut self,
        _frame: &wgpu::SurfaceTexture,
        view: &wgpu::TextureView,
        gfx: &lumino_gfx::Context,
    ) {
        // 从主题获取背景颜色
        let bg_color = self.root.theme().palette().background;
        let clear_color = wgpu::Color {
            r: bg_color.r as f64,
            g: bg_color.g as f64,
            b: bg_color.b as f64,
            a: bg_color.a as f64,
        };

        // 创建命令编码器
        let mut encoder = gfx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("note_render_encoder"),
            });

        // 菜单打开时，禁止更新光标与渲染预览音符
        // 以避免菜单被覆盖或产生误操作
        if !self.root.should_render_preview_note() {
            self.root.update_editor_cursor(None);
        } else {
            // 同步光标位置到编辑器
            self.root.update_editor_cursor(self.cursor_position);
        }

        // 获取需要绘制的音符实例
        let instances = self.root.get_note_instances();

        // 使用逻辑尺寸绘制音符（与 iced 坐标系保持一致）
        let logical_size = self.viewport.logical_size();

        if !instances.is_empty() {
            // 准备渲染（执行计算剔除）
            self.note_renderer.prepare(
                &mut encoder,
                &instances,
                &gfx.device,
                &gfx.queue,
                (logical_size.width, logical_size.height),
            );
        }

        // 开始渲染通道，始终清除背景
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("note_render_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear_color),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        if !instances.is_empty() {
            // 计算 Canvas 区域的裁剪矩形（限制音符只在钢琴卷帘内显示）
            // 转换为物理像素坐标用于裁剪矩形
            let scale = self.viewport.scale_factor();
            let canvas_offset = self.root.editor.canvas_offset;
            let canvas_size = self.root.editor.canvas_size;
            let physical_size = self.viewport.physical_size();

            let scissor_x = ((canvas_offset.x * scale) as u32).min(physical_size.width);
            let scissor_y = ((canvas_offset.y * scale) as u32).min(physical_size.height);
            let scissor_width =
                ((canvas_size.x * scale) as u32).min(physical_size.width.saturating_sub(scissor_x));
            let scissor_height = ((canvas_size.y * scale) as u32)
                .min(physical_size.height.saturating_sub(scissor_y));

            if scissor_width > 0 && scissor_height > 0 {
                self.note_renderer.draw(
                    &mut render_pass,
                    true,
                    Some((scissor_x, scissor_y, scissor_width, scissor_height)),
                );
            }
        }

        // 释放 render_pass 并提交命令
        drop(render_pass);
        gfx.queue.submit(std::iter::once(encoder.finish()));
    }

    pub fn cursor_moved(&mut self, position: winit::dpi::PhysicalPosition<f64>) {
        let logical_pos = conversion::cursor_position(position, self.viewport.scale_factor());
        self.cursor = mouse::Cursor::Available(logical_pos);
        // 存储逻辑坐标（与 iced 保持一致）
        self.cursor_position = Some(logical_pos);

        // 如果正在调整工具栏高度，更新工具栏高度
        if self.is_toolbar_resizing {
            self.root
                .toolbar
                .update_resize_position(logical_pos.y);
            self.cache = std::mem::take(&mut self.cache);
            self.window.request_redraw();
        }
    }

    pub fn handle_events(
        &mut self,
        event: winit::event::WindowEvent,
        modifiers: winit::keyboard::ModifiersState,
    ) {
        use winit::event::WindowEvent::*;

        match &event {
            Resized(_) => self
                .root
                .update(message::Window::maximized(self.window.is_maximized())),
            Focused(r) => self.root.update(message::Window::focused(*r)),
            KeyboardInput { event, .. } => {
                // 处理键盘事件
                if event.state == winit::event::ElementState::Pressed {
                    use winit::keyboard::{KeyCode, PhysicalKey};
                    match event.physical_key {
                        PhysicalKey::Code(KeyCode::Delete)
                        | PhysicalKey::Code(KeyCode::Backspace) => {
                            // 发送删除音符的消息
                            self.root
                                .editor
                                .handle_action(message::EditorAction::DeletePressed);
                            self.window.request_redraw();
                        }
                        _ => {}
                    }
                }
            }
            MouseInput { state, button, .. } => {
                // 全局监听鼠标释放事件，结束工具栏拖拽状态
                if *button == winit::event::MouseButton::Left
                    && *state == winit::event::ElementState::Released
                    && self.is_toolbar_resizing
                {
                    self.is_toolbar_resizing = false;
                    self.root.toolbar.end_resize();
                    // 清除缓存以强制重绘
                    self.cache = std::mem::take(&mut self.cache);
                    self.window.request_redraw();
                }
            }
            _ => (),
        }

        // 将窗口事件映射到 iced 事件
        if let Some(event) =
            conversion::window_event(event, self.window.scale_factor() as f32, modifiers)
        {
            let converted_events = convert_touch_to_mouse(event);
            self.events.extend(converted_events);
        }

        // 处理事件
        if !self.events.is_empty() {
            // 临时取出缓存以避免借用冲突
            let cache = std::mem::take(&mut self.cache);

            let mut interface = UserInterface::build(
                self.root.view(),
                self.viewport.logical_size(),
                cache,
                &mut self.renderer,
            );

            let mut messages = Vec::new();

            let _ = interface.update(
                &self.events,
                self.cursor,
                &mut self.renderer,
                &mut self.clipboard,
                &mut messages,
            );

            self.events.clear();
            self.cache = interface.into_cache();

            // 应用消息
            for message in messages {
                if let message::Message::Window(window::Event::TrafficAction(action)) = &message {
                    self.pending_window_action = Some(action.clone());
                }
                if let message::Message::Window(window::Event::ToggleMaximize) = &message {
                    self.pending_window_action = Some(window::TrafficAction::ToggleMaximize);
                }
                if let message::Message::Window(window::Event::Close) = &message {
                    self.pending_window_action = Some(window::TrafficAction::Close);
                }
                if let message::Message::Window(window::Event::Drag) = &message {
                    self.pending_drag = true;
                }
                // 处理工具栏调整大小事件
                if let message::Message::Toolbar(toolbar::Event::ResizeDragStarted(_)) = &message
                    && let Some(pos) = self.cursor_position
                {
                    self.is_toolbar_resizing = true;
                    self.root.toolbar.start_resize(pos.y);
                }
                if let message::Message::Toolbar(toolbar::Event::ResizeDragEnded) = &message {
                    self.is_toolbar_resizing = false;
                    self.root.toolbar.end_resize();
                }
                self.root.update(message);
            }

            // 清除缓存以确保界面重新构建（特别是侧边栏切换后）
            self.cache = std::mem::take(&mut self.cache);

            self.window.request_redraw();
        }
    }

    /// 获取并清除待处理的窗口动作
    pub fn take_window_action(&mut self) -> Option<window::TrafficAction> {
        self.pending_window_action.take()
    }

    /// 获取并清除待处理的拖动标记
    pub fn take_drag(&mut self) -> bool {
        let drag = self.pending_drag;
        self.pending_drag = false;
        drag
    }

    pub fn update_progress(&mut self, progress: Option<(String, f64)>) {
        self.root.update(message::Message::Progress(progress));
    }

    pub fn update_theme(&mut self, theme: String) {
        self.root.update(window::Event::theme(theme));
        self.cache = std::mem::take(&mut self.cache);
        self.window.request_redraw();
    }

    pub fn settings(&self) -> &settings::SettingsPanel {
        self.root.settings()
    }

    /// 获取并清空待处理的音频动作
    pub fn take_audio_actions(&mut self) -> Vec<message::AudioAction> {
        self.root.take_audio_actions()
    }

    /// 更新音轨列表（从 MIDI 导入）
    /// track_infos: (track_index, track_name, note_count)
    pub fn update_tracks(&mut self, track_infos: &[(usize, Option<String>, u64)]) {
        self.root.update_tracks(track_infos);
        self.cache = std::mem::take(&mut self.cache);
        self.window.request_redraw();
    }

    /// 设置编辑器总 ticks
    pub fn set_total_ticks(&mut self, total_ticks: f32) {
        self.root.set_total_ticks(total_ticks);
        self.cache = std::mem::take(&mut self.cache);
        self.window.request_redraw();
    }

    /// 加载音符到编辑器
    /// notes: (tick, key, length)
    pub fn load_notes(&mut self, notes: &[(f32, u8, f32)]) {
        self.root.load_notes(notes);
        self.cache = std::mem::take(&mut self.cache);
        self.window.request_redraw();
    }

    /// 设置当前音轨
    pub fn set_current_track(&mut self, track_idx: usize) {
        self.root.set_current_track(track_idx);
        self.cache = std::mem::take(&mut self.cache);
        self.window.request_redraw();
    }

    /// 加载指定音轨的音符到编辑器（用于 MIDI 文件）
    /// 这会同时更新当前显示的音符和音轨存储，以便洋葱皮能显示
    pub fn load_track_notes(&mut self, track_idx: usize, notes: &[(f32, u8, f32)]) {
        self.root.load_track_notes(track_idx, notes);
        self.cache = std::mem::take(&mut self.cache);
        self.window.request_redraw();
    }

    /// 预加载音轨音符到 track_notes（仅用于洋葱皮，不显示）
    ///
    /// # 参数
    /// * `track_idx` - 音轨索引
    /// * `notes` - 音符列表，格式为 (tick, key, length)
    pub fn load_track_notes_for_onion_skin(&mut self, track_idx: usize, notes: &[(f32, u8, f32)]) {
        tracing::debug!(
            "UI::load_track_notes_for_onion_skin: track_idx={}, notes_count={}",
            track_idx,
            notes.len()
        );

        // 直接保存到 editor.track_notes，不更新当前显示
        let mut track_notes = Vec::with_capacity(notes.len());
        for (tick, key, length) in notes {
            use crate::editor::note::Note;
            let editor_key = *key as u16;
            track_notes.push(Note::new(*tick, editor_key, *length));
        }

        if !track_notes.is_empty() {
            self.root.editor.track_notes.insert(track_idx, track_notes);
            tracing::debug!(
                "UI::load_track_notes_for_onion_skin: saved {} notes to track_notes[{}]",
                notes.len(),
                track_idx
            );
        }

        // 不需要重绘，因为这些音符是用于洋葱皮的，不是当前显示的
    }

    // ========== 洋葱皮 API ==========

    /// 启用洋葱皮功能
    pub fn enable_onion_skin(&mut self) {
        self.root.editor.enable_onion_skin();
        self.window.request_redraw();
    }

    /// 禁用洋葱皮功能
    pub fn disable_onion_skin(&mut self) {
        self.root.editor.disable_onion_skin();
        self.window.request_redraw();
    }

    /// 切换洋葱皮开关状态
    pub fn toggle_onion_skin(&mut self) {
        self.root.editor.toggle_onion_skin();
        self.window.request_redraw();
    }

    /// 检查洋葱皮是否启用
    pub fn is_onion_skin_enabled(&self) -> bool {
        self.root.editor.is_onion_skin_enabled()
    }

    /// 设置音轨的洋葱皮颜色
    ///
    /// # 参数
    /// * `track_idx` - 音轨索引
    /// * `r`, `g`, `b` - RGB 颜色分量 (0.0 - 1.0)
    /// * `a` - 透明度 (0.0 - 1.0)，可选，默认为当前透明度
    pub fn set_onion_skin_color(
        &mut self,
        track_idx: usize,
        r: f32,
        g: f32,
        b: f32,
        a: Option<f32>,
    ) {
        let color = if let Some(alpha) = a {
            iced_core::Color::from_rgba(r, g, b, alpha)
        } else {
            let alpha = self.root.editor.onion_skin_opacity();
            iced_core::Color::from_rgba(r, g, b, alpha)
        };
        self.root.editor.set_onion_skin_color(track_idx, color);
        self.window.request_redraw();
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
        self.window.request_redraw();
    }

    /// 获取洋葱皮透明度
    pub fn onion_skin_opacity(&self) -> f32 {
        self.root.editor.onion_skin_opacity()
    }

    /// 设置是否显示所有音轨的洋葱皮
    pub fn set_onion_skin_show_all(&mut self, show_all: bool) {
        self.root.editor.set_onion_skin_show_all(show_all);
        self.window.request_redraw();
    }

    /// 添加音轨到洋葱皮显示列表
    pub fn add_onion_skin_track(&mut self, track_idx: usize) {
        self.root.editor.add_onion_skin_track(track_idx);
        self.window.request_redraw();
    }

    /// 从洋葱皮显示列表移除音轨
    pub fn remove_onion_skin_track(&mut self, track_idx: usize) {
        self.root.editor.remove_onion_skin_track(track_idx);
        self.window.request_redraw();
    }

    /// 清空编辑器（用于新建工程）
    pub fn clear_editor(&mut self) {
        self.root.editor.notes.clear();
        self.root.editor.track_notes.clear();
        self.root.editor.current_track = 0;
        self.root.editor.grid_cache.clear();
        self.cache = std::mem::take(&mut self.cache);
        self.window.request_redraw();
        tracing::info!("UI: 编辑器已清空");
    }

    /// 获取编辑器中的所有音符数据（用于保存）
    ///
    /// 返回 (track_idx, notes) 列表，其中 notes 格式为 (tick, key, length)
    pub fn get_editor_notes(&self) -> Vec<(usize, Vec<(f32, u8, f32)>)> {
        let mut result = Vec::new();

        // 先保存当前音轨的音符
        if !self.root.editor.notes.is_empty() {
            let current_notes: Vec<(f32, u8, f32)> = self
                .root
                .editor
                .notes
                .iter()
                .map(|n| (n.tick, n.key as u8, n.length))
                .collect();
            result.push((self.root.editor.current_track, current_notes));
        }

        // 添加其他音轨的音符
        for (&track_idx, notes) in &self.root.editor.track_notes {
            if track_idx != self.root.editor.current_track {
                let track_notes: Vec<(f32, u8, f32)> = notes
                    .iter()
                    .map(|n| (n.tick, n.key as u8, n.length))
                    .collect();
                result.push((track_idx, track_notes));
            }
        }

        result
    }

    /// 获取编辑器中的音符数量（用于判断是否有内容）
    pub fn get_editor_note_count(&self) -> usize {
        let current_count = self.root.editor.notes.len();
        let track_notes_count: usize = self.root.editor.track_notes.values().map(|v| v.len()).sum();
        current_count + track_notes_count
    }
}

/// 对话框结果
#[derive(Debug, Clone)]
pub enum DialogResult {
    CustomPrecision { numerator: String, denominator: String },
}

impl Host {
    /// 设置自定义精度对话框是否打开（用于独立对话框窗口）
    pub fn set_custom_precision_dialog_open(&mut self, open: bool) {
        self.root.set_custom_precision_dialog_open(open);
        self.cache = std::mem::take(&mut self.cache);
        self.window.request_redraw();
    }

    /// 获取并清空对话框结果
    pub fn take_dialog_result(&mut self) -> Option<DialogResult> {
        self.root.take_dialog_result()
    }

    /// 设置自定义精度值（用于独立对话框窗口）
    pub fn set_custom_precision(&mut self, ticks: f32) {
        self.root.set_custom_precision(ticks);
        self.cache = std::mem::take(&mut self.cache);
        self.window.request_redraw();
    }
}

/// 将触摸事件转换为鼠标事件（兼容性处理）
fn convert_touch_to_mouse(event: Event) -> Vec<Event> {
    match event {
        Event::Touch(touch_event) => match touch_event {
            touch::Event::FingerPressed { position, .. } => {
                vec![
                    event,
                    Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                    Event::Mouse(mouse::Event::CursorMoved { position }),
                ]
            }
            touch::Event::FingerLifted { position, .. } => {
                vec![
                    event,
                    Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                    Event::Mouse(mouse::Event::CursorMoved { position }),
                ]
            }
            touch::Event::FingerMoved { position, .. } => {
                vec![event, Event::Mouse(mouse::Event::CursorMoved { position })]
            }
            _ => vec![event],
        },
        _ => vec![event],
    }
}
