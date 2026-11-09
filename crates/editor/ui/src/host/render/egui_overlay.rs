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

mod actions;
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
    /// 本帧色板（跟随窗口主题）。
    palette: lumino_ui_egui::theme::Palette,
    /// 钢琴瀑布流离屏纹理（面板可见且已渲染时才注册）。
    waterfall: Option<std::sync::Arc<wgpu::TextureView>>,
    /// 音频导出面板（整块接管右侧内容区）。
    audio_export: Option<lumino_ui_egui::chrome::AudioExportPanelModel>,
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
    fn draw(&mut self, frame: OverlayFrame<'_>) -> (Vec<Rect>, bool) {
        self.frames = self.frames.wrapping_add(1);

        // 外壳是不透明 band，颜色必须跟随当前主题（默认配置是 Light）。
        lumino_ui_egui::theme::set_palette(frame.palette);

        // 钢琴瀑布流：把 lumino-gfx 的离屏纹理注册成 egui 外部纹理。
        // 面板关闭 / 纹理未就绪时注销，避免 egui 引用已释放的视图。
        match &frame.waterfall {
            Some(view) => self.painter.set_external_texture(
                frame.device,
                lumino_ui_egui::chrome::WATERFALL_TEXTURE_ID,
                view,
            ),
            None => self
                .painter
                .clear_external_texture(lumino_ui_egui::chrome::WATERFALL_TEXTURE_ID),
        }

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

        // `run_ui` 不返回闭包值：动作统一由 `OverlayActions::take` 取走，
        // 这里只要占用矩形与文本焦点。
        let mut blocked = chrome::menu::take_blocked(&ctx);
        // 音频导出面板不透明覆盖右侧内容区：整块进入占用列表，点击不再落到 iced。
        if let Some(panel) = &frame.audio_export {
            blocked.push(panel.rect);
        }

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
        (blocked, self.ctx.text_edit_focused())
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
        let logical_height = size.height as f32 / pixels_per_point;
        let model = self.root.egui_chrome_model_for(logical_width);
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
        // 瀑布流面板可见且纹理已渲染时，把视图交给 egui 采样（iced 侧同位置
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
        let events = self.egui_input.take();
        let focused = self.egui_input.focused();
        // Device/Queue 是 Arc 句柄，克隆避免与 `egui_overlay` 的可变借用冲突。
        let device = self.render_ctx.device.clone();
        let queue = self.render_ctx.queue.clone();

        let overlay = self
            .render_ctx
            .egui_overlay
            .get_or_insert_with(|| EguiOverlay::new(&device, format));
        let (blocked, wants_keyboard) = overlay.draw(OverlayFrame {
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
            events,
            focused,
        });
        // 各通道动作统一取走并分发（`actions.rs`）。
        let actions = actions::OverlayActions::take(&overlay.ctx);
        // egui 的弹层（ComboBox / Tooltip）首帧是「测量 pass」，不产生绘制图元：
        // 它会在本帧请求一次重绘，必须立刻再排一帧，否则下拉菜单要等到下一次
        // 鼠标事件才出现。
        if overlay.ctx.has_requested_repaint() {
            self.window_ctx.window.request_redraw();
        }
        // 下一帧点击命中这些矩形时，事件只给 egui，不再进 iced 队列。
        self.egui_blocked = blocked;
        self.egui_input.set_wants_keyboard(wants_keyboard);
        self.apply_overlay_actions(actions, logical_width);
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
        let scale = self.render_ctx.viewport.scale_factor();
        let scale = if scale > 0.0 { scale } else { 1.0 };
        // 与渲染用的 `pixels_per_point` 同源：两者必须一致，否则命中坐标
        // 与实际绘制的控件位置会按比例错开。
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
                // 设置 / 保存确认窗口由 egui 整窗绘制：按键只给 egui，避免同一击
                // 同时触发下层 iced 控件（例如云管理「添加连接」打开两个窗口）。
                // 其余对话框仍依赖 iced let-through，不消费。
                self.dialog_input_exclusive() || self.egui_pointer_blocked()
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

    /// 当前窗口是否为**整窗由 egui 绘制**的对话框（设置 / 保存确认），
    /// 这类窗口必须消费鼠标按键，不能再落到 iced。
    fn dialog_input_exclusive(&self) -> bool {
        if !self.root.state.is_dialog_window {
            return false;
        }
        match self.root.state.dialog_type {
            crate::state::root_state::DialogType::Settings
            | crate::state::root_state::DialogType::SaveConfirm => true,
            // 工程设置 / 撤销历史 / 批量编辑的顶部 30px 自制标题栏（红绿灯 / 拖动）
            // 仍由 iced 绘制：该区域内的按键不消费，其余内容区只给 egui。
            crate::state::root_state::DialogType::ProjectSettings
            | crate::state::root_state::DialogType::History
            | crate::state::root_state::DialogType::BatchEdit
            | crate::state::root_state::DialogType::SpeedChange
            | crate::state::root_state::DialogType::CustomPrecision
            | crate::state::root_state::DialogType::Help
            | crate::state::root_state::DialogType::CloudConnect
            | crate::state::root_state::DialogType::MemoryMonitor
            | crate::state::root_state::DialogType::DrawSettings
            | crate::state::root_state::DialogType::LoadConfirm
            | crate::state::root_state::DialogType::BrushSettings
            | crate::state::root_state::DialogType::RecoverTrack => {
                self.root.settings.synth.use_native_titlebar
                    || self
                        .egui_input
                        .pointer
                        .is_none_or(|pos| pos.y >= lumino_ui_egui::layout::TITLEBAR_H)
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

/// 环境变量开关（只读一次；`0` 视为关闭）。
pub(crate) fn overlay_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED
        .get_or_init(|| std::env::var_os("LUMINO_EGUI_OVERLAY").is_some_and(|value| value != "0"))
}
