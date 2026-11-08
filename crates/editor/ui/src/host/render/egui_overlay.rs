//! 实验性 egui 外壳叠加层（`LUMINO_EGUI_OVERLAY=1` 时启用）。
//!
//! 目的：在真实窗口 surface / 真实 `Device` 上验证 `lumino-egui-bridge`，
//! 并把生产状态（标题栏 / 状态栏）逐步搬到 egui。默认关闭，不改变现有画面。
//!
//! 当前接管范围：
//! - 标题栏 30px：底色随窗口聚焦变化、真实菜单标签（按语言）、logo、窗口按钮；
//! - 状态栏 20px：左侧提示文本（悬停说明 / 音符数 / 就绪）、CPU / MEM / FPS、右侧文本。
//!
//! 只绘制这些 band，其余区域保持 iced 画面；band 用不透明底色完全覆盖同位置
//! 的 iced 版本，因此不会出现文字重影。
//!
//! # 输入
//!
//! 指针移动 / 滚轮 / 修饰键 / 焦点同步给 egui（悬停高亮、`ScrollArea` 滚动、
//! 菜单点击都要用到）。鼠标按键只在**菜单占用区**（标题栏菜单标签、已展开的
//! 下拉/子面板）内消费；其余区域仍把事件交给 iced，动作由 iced 执行。

use egui::{Rect, Vec2};
use iced_wgpu::wgpu;
use iced_winit::winit;
use lumino_egui_bridge::{Painter, Screen};
use lumino_ui_egui::chrome::{self, ChromeModel};
use lumino_ui_egui::icons::IconCache;

use crate::host::Host;

mod input;
pub(crate) use input::OverlayInput;
use input::pointer_button;

/// 叠加层状态（首次启用时创建，跨帧保留纹理与管线）。
pub(crate) struct EguiOverlay {
    /// egui 上下文。
    ctx: egui::Context,
    /// 图标纹理缓存（真实 SVG，按物理像素缓存）。
    icons: IconCache,
    /// wgpu27 绘制桥。
    painter: Painter,
    /// 叠加层已绘制帧数。
    frames: u32,
}

/// 一帧叠加层绘制输入。
struct OverlayFrame<'a> {
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    view: &'a wgpu::TextureView,
    format: wgpu::TextureFormat,
    size_px: [u32; 2],
    pixels_per_point: f32,
    app_frame: u32,
    model: &'a ChromeModel,
    /// 本帧要提交给 egui 的指针 / 滚轮 / 修饰键 / 焦点事件。
    events: Vec<egui::Event>,
    /// 窗口是否持有焦点。
    focused: bool,
}

impl EguiOverlay {
    /// 用当前 surface 格式创建叠加层。
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        tracing::info!("egui 外壳叠加层已启用：目标格式 {format:?}");
        let ctx = egui::Context::default();
        lumino_ui_egui::theme::apply(&ctx);
        lumino_ui_egui::fonts::install(&ctx);
        Self {
            ctx,
            icons: IconCache::default(),
            painter: Painter::new(device, format),
            frames: 0,
        }
    }

    /// 渲染一帧外壳并提交到目标纹理。
    ///
    /// 返回值末尾的 `bool` 表示 egui 本帧是否需要键盘输入（文本编辑中），
    /// 调用方据此决定下一帧按键事件是否只给 egui。
    fn draw(
        &mut self,
        frame: OverlayFrame<'_>,
    ) -> (
        Option<usize>,
        Option<chrome::ToolbarAction>,
        Option<chrome::right_panel::RightSidebarAction>,
        Vec<Rect>,
        bool,
    ) {
        self.frames = self.frames.wrapping_add(1);

        // 让 egui 以真实 DPI 布局与栅格化字体：否则字体图集按 1× 生成、
        // 再由着色器放大会发虚（几何位置不变，但字形明显更糊）。
        self.ctx.set_pixels_per_point(frame.pixels_per_point);

        let logical = [
            frame.size_px[0] as f32 / frame.pixels_per_point,
            frame.size_px[1] as f32 / frame.pixels_per_point,
        ];
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(logical[0], logical[1]),
            )),
            events: frame.events,
            focused: frame.focused,
            ..Default::default()
        };

        let ctx = self.ctx.clone();
        let icons = &mut self.icons;
        let output = ctx.run_ui(input, |ui| {
            chrome::show_bands(ui, frame.model, icons);
        });

        // `run_ui` 不返回闭包值：菜单/工具栏点击与占用矩形通过 egui 内存回传。
        let menu_action = chrome::menu::take_action(&ctx);
        let toolbar_action = chrome::toolbar_popup::take_action(&ctx);
        let panel_action = chrome::right_panel::take_action(&ctx);
        let blocked = chrome::menu::take_blocked(&ctx);

        let mut textures = output.textures_delta;
        self.painter
            .update_textures(frame.device, frame.queue, &mut textures);
        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);

        let mut encoder = frame
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("lumino_egui_overlay_encoder"),
            });
        let drawn = self.painter.render(
            frame.device,
            frame.queue,
            &mut encoder,
            frame.view,
            Screen {
                size_px: frame.size_px,
                pixels_per_point: frame.pixels_per_point,
            },
            &primitives,
        );
        frame.queue.submit(Some(encoder.finish()));
        if drawn == 0 {
            tracing::debug!("egui 外壳本帧没有可绘制图元");
        }
        if self.frames == 1 {
            tracing::info!(
                "egui 外壳首帧：size={:?} ppp={} format={:?} app_frame={} primitives={} drawn={drawn}",
                frame.size_px,
                frame.pixels_per_point,
                frame.format,
                frame.app_frame,
                primitives.len(),
            );
        }
        (
            menu_action,
            toolbar_action,
            panel_action,
            blocked,
            self.ctx.text_edit_focused(),
        )
    }
}

impl Host {
    /// 在 iced `present` 之后叠加 egui 外壳（默认关闭）。
    pub(super) fn render_egui_overlay(
        &mut self,
        frame: &wgpu::SurfaceTexture,
        view: &wgpu::TextureView,
    ) {
        if !overlay_enabled() {
            return;
        }

        let format = frame.texture.format();
        let size = self.render_ctx.viewport.physical_size();
        let pixels_per_point = self.render_ctx.viewport.scale_factor();
        let app_frame = self.frame_count;
        // 工具栏折叠（溢出菜单）依赖实际可用宽度：与生产 `responsive` 同口径。
        let logical_width = size.width as f32 / pixels_per_point;
        let model = self.root.egui_chrome_model_for(logical_width);
        // 取走本帧累积的指针 / 滚轮 / 修饰键 / 焦点事件。
        let events = self.egui_input.take();
        let focused = self.egui_input.focused();
        // Device/Queue 是 Arc 句柄，克隆避免与 `egui_overlay` 的可变借用冲突。
        let device = self.render_ctx.device.clone();
        let queue = self.render_ctx.queue.clone();

        let overlay = self
            .render_ctx
            .egui_overlay
            .get_or_insert_with(|| EguiOverlay::new(&device, format));
        let (menu_action, toolbar_action, panel_action, blocked, wants_keyboard) =
            overlay.draw(OverlayFrame {
                device: &device,
                queue: &queue,
                view,
                format,
                size_px: [size.width, size.height],
                pixels_per_point,
                app_frame,
                model: &model,
                events,
                focused,
            });
        // 设置对话框动作：菜单切换 / 表单变更 / 确认（独立 OS 窗口同样是叠加层宿主）。
        let settings_action = lumino_ui_egui::settings::take_action(&overlay.ctx);
        // egui 的弹层（ComboBox / Tooltip）首帧是「测量 pass」，不产生绘制图元：
        // 它会在本帧请求一次重绘，必须立刻再排一帧，否则下拉菜单要等到下一次
        // 鼠标事件才出现。
        if overlay.ctx.has_requested_repaint() {
            self.window_ctx.window.request_redraw();
        }
        // 下一帧点击命中这些矩形时，事件只给 egui，不再进 iced 队列。
        self.egui_blocked = blocked;
        self.egui_input.set_wants_keyboard(wants_keyboard);
        if let Some(index) = menu_action
            && let Some(message) = self.root.menu_action_message(index)
        {
            self.ui_dirty = true;
            self.window_ctx.window.request_redraw();
            let _ = self.process_message(message);
        }
        if let Some(action) = settings_action
            && let Some(message) = self.root.settings_action_message(&action)
        {
            tracing::info!("egui 设置动作：{action:?}");
            self.ui_dirty = true;
            self.window_ctx.window.request_redraw();
            let _ = self.process_message(message);
        }
        if let Some(action) = panel_action
            && let Some(message) = self.root.right_sidebar_action_message(&action)
        {
            tracing::info!("egui 右侧栏动作：{action:?}");
            self.ui_dirty = true;
            self.window_ctx.window.request_redraw();
            let _ = self.process_message(message);
        }
        if let Some(action) = toolbar_action {
            // 溢出项需要按「同一视口宽度」重算折叠结果，单独解析。
            let message = match &action {
                chrome::ToolbarAction::OverflowItem(index) => {
                    self.root.overflow_action_message(*index, logical_width)
                }
                other => self.root.toolbar_action_message(other),
            };
            if let Some(message) = message {
                tracing::info!("egui 工具栏动作：{action:?}");
                self.ui_dirty = true;
                self.window_ctx.window.request_redraw();
                let _ = self.process_message(message);
            }
        }
    }

    /// 把 winit 指针 / 滚轮 / 修饰键 / 焦点事件转成 egui 事件。
    ///
    /// 返回 `true` 表示该事件落在 egui 的菜单占用区（标题栏/下拉面板）内，
    /// 调用方必须**不**再把它交给 iced——否则会同时触发下层工具栏/侧栏的点击。
    /// 其余区域保持「只观察不消费」，动作仍由 iced 执行。
    pub(crate) fn capture_egui_overlay_input(&mut self, event: &winit::event::WindowEvent) -> bool {
        use winit::event::{MouseScrollDelta, WindowEvent};

        if !overlay_enabled() {
            return false;
        }
        let scale = self.window_ctx.window.scale_factor() as f32;
        let scale = if scale > 0.0 { scale } else { 1.0 };
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.egui_input.pointer_moved(egui::pos2(
                    position.x as f32 / scale,
                    position.y as f32 / scale,
                ));
                false
            }
            WindowEvent::CursorLeft { .. } => {
                self.egui_input.pointer_left();
                false
            }
            WindowEvent::MouseWheel { delta, .. } => {
                match delta {
                    MouseScrollDelta::LineDelta(x, y) => {
                        self.egui_input
                            .wheel(egui::MouseWheelUnit::Line, egui::vec2(*x, *y));
                    }
                    MouseScrollDelta::PixelDelta(position) => {
                        self.egui_input.wheel(
                            egui::MouseWheelUnit::Point,
                            egui::vec2(position.x as f32, position.y as f32) / scale,
                        );
                    }
                }
                self.egui_pointer_blocked()
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let Some(button) = pointer_button(*button) else {
                    return false;
                };
                self.egui_input
                    .button(button, *state == winit::event::ElementState::Pressed);
                // 设置窗口由 egui 整窗绘制：按键只给 egui，避免同一击同时触发
                // 下层 iced 控件（例如云管理「添加连接」打开两个窗口）。
                // 保存确认等其它对话框的按钮仍依赖 let-through，暂不消费。
                self.settings_dialog_window() || self.egui_pointer_blocked()
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                let state = modifiers.state();
                let super_key = state.contains(winit::keyboard::ModifiersState::SUPER);
                let control = state.contains(winit::keyboard::ModifiersState::CONTROL);
                self.egui_input.set_modifiers(egui::Modifiers {
                    alt: state.contains(winit::keyboard::ModifiersState::ALT),
                    ctrl: control,
                    shift: state.contains(winit::keyboard::ModifiersState::SHIFT),
                    mac_cmd: super_key,
                    // egui 语义：command = ⌘（macOS）/ Ctrl（其他平台）。
                    command: if cfg!(target_os = "macos") {
                        super_key
                    } else {
                        control
                    },
                });
                false
            }
            WindowEvent::KeyboardInput { event, .. } => {
                self.egui_input.key(event);
                // 只有 egui 正在编辑文本时才消费：否则快捷键仍要落到 iced。
                self.egui_input.wants_keyboard()
            }
            WindowEvent::Ime(winit::event::Ime::Commit(text)) => {
                self.egui_input.text(text);
                self.egui_input.wants_keyboard()
            }
            WindowEvent::Focused(focused) => {
                self.egui_input.focus(*focused);
                false
            }
            _ => false,
        }
    }

    /// 当前窗口是否为设置对话框（全部 10 页已由 egui 绘制，可整窗消费输入）。
    fn settings_dialog_window(&self) -> bool {
        self.root.state.is_dialog_window
            && matches!(
                self.root.state.dialog_type,
                crate::state::root_state::DialogType::Settings
            )
    }

    /// 当前指针是否落在 egui 菜单占用区（标题栏标签 / 下拉面板）。
    fn egui_pointer_blocked(&self) -> bool {
        self.egui_input
            .pointer
            .is_some_and(|pos| self.egui_blocked.iter().any(|rect| rect.contains(pos)))
    }
}

/// 环境变量开关（只读一次；`0` 视为关闭）。
pub(crate) fn overlay_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED
        .get_or_init(|| std::env::var_os("LUMINO_EGUI_OVERLAY").is_some_and(|value| value != "0"))
}
