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

/// 叠加层输入缓冲：只收「影响显示态」的事件（悬停 / 滚轮 / 修饰键 / 焦点）。
///
/// 与 iced 的事件队列同策略：连续指针移动只保留最后一次，避免一帧内堆积
/// 成百上千个中间位置。
pub(crate) struct OverlayInput {
    /// 待提交给 egui 的事件。
    events: Vec<egui::Event>,
    /// 当前修饰键状态（滚轮/按键事件需要随附）。
    modifiers: egui::Modifiers,
    /// 窗口是否持有焦点（`RawInput::focused`）。
    focused: bool,
    /// 最近一次指针位置（逻辑点），鼠标按键事件需要。
    pointer: Option<egui::Pos2>,
}

impl Default for OverlayInput {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            modifiers: egui::Modifiers::default(),
            // 与 egui 的 `RawInput::focused` 默认值一致：窗口可见即可交互。
            focused: true,
            pointer: None,
        }
    }
}

impl OverlayInput {
    /// 指针移动到窗口内（逻辑点）。
    fn pointer_moved(&mut self, position: egui::Pos2) {
        self.pointer = Some(position);
        if matches!(self.events.last(), Some(egui::Event::PointerMoved(_))) {
            self.events.pop();
        }
        self.events.push(egui::Event::PointerMoved(position));
    }

    /// 指针离开窗口。
    fn pointer_left(&mut self) {
        self.pointer = None;
        self.events.push(egui::Event::PointerGone);
    }

    /// 鼠标按键（左/右/中）。
    fn button(&mut self, button: egui::PointerButton, pressed: bool) {
        let Some(pos) = self.pointer else {
            return;
        };
        self.events.push(egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: self.modifiers,
        });
    }

    /// 滚轮：单位沿用 winit 语义（行 / 像素），换算交给 egui。
    fn wheel(&mut self, unit: egui::MouseWheelUnit, delta: egui::Vec2) {
        self.events.push(egui::Event::MouseWheel {
            unit,
            delta,
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
            modifiers: self.modifiers,
        });
    }

    /// 修饰键变化（只有真的变化才入队）。
    fn set_modifiers(&mut self, modifiers: egui::Modifiers) {
        if self.modifiers == modifiers {
            return;
        }
        self.modifiers = modifiers;
        self.events.push(egui::Event::ModifiersChanged(modifiers));
    }

    /// 窗口焦点变化。
    fn focus(&mut self, focused: bool) {
        if self.focused == focused {
            return;
        }
        self.focused = focused;
        self.events.push(egui::Event::WindowFocused(focused));
    }

    /// 取走本帧事件（缓冲清空）。
    fn take(&mut self) -> Vec<egui::Event> {
        std::mem::take(&mut self.events)
    }

    /// 窗口是否持有焦点。
    fn focused(&self) -> bool {
        self.focused
    }
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
    fn draw(
        &mut self,
        frame: OverlayFrame<'_>,
    ) -> (Option<usize>, Option<chrome::ToolbarAction>, Vec<Rect>) {
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
        (menu_action, toolbar_action, blocked)
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
        let (menu_action, toolbar_action, blocked) = overlay.draw(OverlayFrame {
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
        // 下一帧点击命中这些矩形时，事件只给 egui，不再进 iced 队列。
        self.egui_blocked = blocked;
        if let Some(index) = menu_action
            && let Some(message) = self.root.menu_action_message(index)
        {
            self.ui_dirty = true;
            self.window_ctx.window.request_redraw();
            let _ = self.process_message(message);
        }
        if let Some(action) = toolbar_action {
            // 溢出项需要按「同一视口宽度」重算折叠结果，单独解析。
            let message = match action {
                chrome::ToolbarAction::OverflowItem(index) => {
                    self.root.overflow_action_message(index, logical_width)
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
                self.egui_pointer_blocked()
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
            WindowEvent::Focused(focused) => {
                self.egui_input.focus(*focused);
                false
            }
            _ => false,
        }
    }

    /// 当前指针是否落在 egui 菜单占用区（标题栏标签 / 下拉面板）。
    fn egui_pointer_blocked(&self) -> bool {
        self.egui_input
            .pointer
            .is_some_and(|pos| self.egui_blocked.iter().any(|rect| rect.contains(pos)))
    }
}

/// winit 鼠标键 → egui 指针键（其余侧键不参与菜单）。
fn pointer_button(button: winit::event::MouseButton) -> Option<egui::PointerButton> {
    use winit::event::MouseButton;
    match button {
        MouseButton::Left => Some(egui::PointerButton::Primary),
        MouseButton::Right => Some(egui::PointerButton::Secondary),
        MouseButton::Middle => Some(egui::PointerButton::Middle),
        _ => None,
    }
}

/// 环境变量开关（只读一次；`0` 视为关闭）。
pub(crate) fn overlay_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED
        .get_or_init(|| std::env::var_os("LUMINO_EGUI_OVERLAY").is_some_and(|value| value != "0"))
}
