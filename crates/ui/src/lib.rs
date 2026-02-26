mod editor;
pub mod message;
mod resources;
mod root;
mod sidebar;
mod statusbar;
mod titlebar;
pub mod window;

pub(crate) use lumino_core::storage::config;
pub(crate) use root::{Element, Message};

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

use iced_core::{Event, Font, Pixels, Size, Theme, mouse, renderer, touch};

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
    /// wgpu 音符渲染器
    note_renderer: NoteRenderer,
    /// 逻辑光标位置（用于音符预览）
    cursor_position: Option<iced_core::Point>,
    /// 上一次帧时间（用于计算 FPS）
    last_frame_time: Instant,
    /// 上一次 FPS 更新时间
    last_fps_update: Instant,
    /// 帧计数器
    frame_count: u32,
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

        // 初始化 iced
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
                root::Root::new(&ui_config.theme)
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

        // 第一步：用 wgpu 渲染音符（在 UI 之下）
        self.render_notes(frame, view, gfx);

        // 第二步：渲染 iced UI
        self.render_iced_ui(frame, view);
    }

    /// 渲染 iced UI
    fn render_iced_ui(&mut self, frame: &wgpu::SurfaceTexture, texture_view: &wgpu::TextureView) {
        // 先构建 view（这会借用 root）
        let root_view = self.root.view();

        let mut interface = UserInterface::build(
            root_view,
            self.viewport.logical_size(),
            std::mem::take(&mut self.cache),
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

        // 绘制界面（在释放 root 借用之前）
        let theme = self.root.theme();
        interface.draw(
            &mut self.renderer,
            &theme,
            &renderer::Style::default(),
            self.cursor,
        );
        self.cache = interface.into_cache();

        self.renderer
            .present(None, frame.texture.format(), texture_view, &self.viewport);

        // 处理消息（在 interface 被释放之后，root 不再被借用）
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
        // 获取背景颜色
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

        // 菜单打开时，禁止更新光标与渲染预览音符（避免菜单被覆盖/误操作）
        if !self.root.should_render_preview_note() {
            self.root.update_editor_cursor(None);
        } else {
            // 同步光标位置到 editor
            self.root.update_editor_cursor(self.cursor_position);
        }

        // 获取需要绘制的音符实例
        let instances = self.root.get_note_instances();

        // 使用逻辑尺寸绘制音符（与 iced 坐标系一致）
        let logical_size = self.viewport.logical_size();

        if !instances.is_empty() {
            // 准备绘制（执行 Compute Culling）
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
                    load: wgpu::LoadOp::Clear(clear_color), // 清除背景
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        if !instances.is_empty() {
            // 计算 Canvas 区域的裁剪矩形（限制音符只在卷帘内显示）
            // 转换为物理像素坐标用于 scissor rect
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

        // 释放 render_pass，提交命令
        drop(render_pass);
        gfx.queue.submit(std::iter::once(encoder.finish()));
    }

    pub fn cursor_moved(&mut self, position: winit::dpi::PhysicalPosition<f64>) {
        let logical_pos = conversion::cursor_position(position, self.viewport.scale_factor());
        self.cursor = mouse::Cursor::Available(logical_pos);
        // 存储逻辑坐标（与 iced 一致）
        self.cursor_position = Some(logical_pos);
    }

    pub fn handle_events(
        &mut self,
        event: winit::event::WindowEvent,
        modifiers: winit::keyboard::ModifiersState,
    ) {
        use winit::event::WindowEvent::*;

        match event {
            Resized(_) => self
                .root
                .update(message::Window::maximized(self.window.is_maximized())),
            Focused(r) => self.root.update(message::Window::focused(r)),
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
            let mut interface = UserInterface::build(
                self.root.view(),
                self.viewport.logical_size(),
                std::mem::take(&mut self.cache),
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
                if let message::Message::Window(window::Event::Drag) = &message {
                    self.pending_drag = true;
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

    /// 获取并清除待处理的拖动标志
    pub fn take_drag(&mut self) -> bool {
        let drag = self.pending_drag;
        self.pending_drag = false;
        drag
    }

    pub fn update_progress(&mut self, progress: Option<(String, f64)>) {
        self.root.update(message::Message::Progress(progress));
    }

    pub fn update_theme(&mut self, theme: String) {
        self.root.update(message::Window::theme(theme));
        self.cache = std::mem::take(&mut self.cache);
        self.window.request_redraw();
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
