//! egui 整窗外壳（唯一生产 UI 层）。
//!
//! 在真实窗口 surface / 真实 `Device` 上运行 `lumino-egui-bridge`；外壳 band、
//! 对话框、面板与编辑器骨架全部由 egui 绘制，编辑器画布底层采样 gfx 离屏纹理。
//!
//! # 输入
//!
//! 指针 / 滚轮 / 修饰键 / 焦点同步给 egui；浮层（菜单 / 弹层 / 模态）的命中
//! 由 `ctx.layer_id_at` 判定，画布输入在浮层覆盖时让位。

use egui::Rect;
use lumino_egui_bridge::{Painter, Screen};
use lumino_ui_egui::chrome::{self, ChromeModel};
use lumino_ui_egui::icons::IconCache;
use wgpu;

use crate::host::Host;

mod actions;
mod canvas;
mod clock;
pub(crate) mod input;
pub use input::OverlayInput;

/// 叠加层状态（首次启用时创建，跨帧保留纹理与管线）。
pub(crate) struct EguiOverlay {
    /// egui 上下文。
    ctx: egui::Context,
    /// 图标纹理缓存（真实 SVG，按物理像素缓存）。
    icons: IconCache,
    /// wgpu27 绘制桥。
    painter: Painter,
    /// 帧时钟：egui 的点击 / 长按 / 双击判定都是**时间**语义，
    /// 每帧必须把真实秒数填进 `RawInput::time`（见 `clock` 模块注释）。
    clock: clock::FrameClock,
    /// 诊断开关：`LUMINO_EGUI_LEGACY_CLOCK=1` 时退回「不填 `time`」的旧行为。
    ///
    /// 只用于真机 A/B 复现（同一份二进制对比修复前后），正常启动必须为 `false`。
    /// 构造时读一次，避免每帧查询环境变量。
    legacy_clock: bool,
    /// 诊断开关：`LUMINO_CLOCK_DIAG=1` 时每帧比对 egui 内部时钟与真实时间。
    clock_diag: bool,
    /// 叠加层已绘制帧数。
    frames: u32,
    /// 本帧 egui 占用区（菜单 / 弹层 / 面板 / 模态，逻辑点）。
    ///
    /// 命中判定归 egui 自己所有：`Host` 不再保存第二份矩形清单，只查询
    /// [`EguiOverlay::pointer_blocked`]。
    blocked: Vec<Rect>,
}

/// 一帧叠加层绘制结果（渲染之外的宿主副作用）。
pub(crate) struct OverlayOutput {
    /// egui 请求的系统光标。
    pub cursor: egui::CursorIcon,
    /// 文本编辑中的 IME 锚点（`Some` = 应开启系统输入法）。
    pub ime: Option<egui::output::IMEOutput>,
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
    /// 渲染器预览 / 全屏瀑布流的离屏纹理（二者互斥；就绪时才注册）。
    render_preview: Option<std::sync::Arc<wgpu::TextureView>>,
    /// 钢琴瀑布流离屏纹理（面板可见且已渲染时才注册）。
    waterfall: Option<std::sync::Arc<wgpu::TextureView>>,
    /// 音频导出面板（整块接管右侧内容区）。
    audio_export: Option<lumino_ui_egui::chrome::AudioExportPanelModel>,
    /// 形状抽屉左侧面板。
    shape_drawer: Option<lumino_ui_egui::chrome::ShapeDrawerModel>,
    /// 形状抽屉右侧画布（egui painter 版）。
    shape_canvas: Option<lumino_ui_egui::shape_canvas::ShapeCanvasModel>,
    /// 视频导出面板（整块替换右侧内容区）。
    video_export: Option<lumino_ui_egui::chrome::VideoExportPanelModel>,
    /// 渲染器入口面板（轨道 / 16:9 预览 / 时间轴卡片 / 导出设置）。
    renderer_panel: Option<lumino_ui_egui::renderer_panel::RendererPanelModel>,
    /// 编辑器画布（卷帘 / 走带）离屏纹理；`Some` 时 egui 整窗采样它作底层。
    editor_texture: Option<std::sync::Arc<wgpu::TextureView>>,
    /// 走带标尺模型；`Some` 且开关打开时由 egui 绘制。
    arrangement: Option<lumino_ui_egui::chrome::ArrangementModel>,
    /// 遮罩参数面板；`Some` 时由 egui 绘制并接管输入。
    /// 右下角 Toast 卡片（空 = 不显示）。
    toast: lumino_ui_egui::chrome::ToastModel,
    /// 首次使用帮助提示（None = 不显示）。
    help_tip: Option<lumino_ui_egui::chrome::HelpTipModel>,
    /// 「关于」页 logo 彩蛋飞行层（None = 不在飞行中）。
    egg: Option<lumino_ui_egui::chrome::EggModel>,
    /// 钢琴卷帘右键菜单（None = 未打开）。
    piano_roll_menu: Option<lumino_ui_egui::piano_roll_context_menu::PianoRollMenuModel>,
    /// 编辑器悬浮 √× 按钮（空 = 不显示）。
    editor_buttons: lumino_ui_egui::editor_action_buttons::EditorButtonsModel,
    /// 框选框（空 = 不显示）。
    marquee: lumino_ui_egui::marquee::MarqueeModel,
    /// 形状工具预览（空 = 不显示）。
    shape_preview: lumino_ui_egui::shape_preview::ShapePreviewModel,
    /// 曲线工具预览（空 = 不显示）。
    curve_preview: lumino_ui_egui::curve_preview::CurvePreviewModel,
    /// 协作远程光标（空 = 无人在线）。
    remote_cursors: lumino_ui_egui::remote_cursors::RemoteCursorModel,
    /// 编辑器叠加层预览（切割线 / 刀框 / 角度柔化）。
    editor_overlay: lumino_ui_egui::editor_overlay::EditorOverlayModel,
    /// 图片转 MIDI 区域框。
    i2m_region: lumino_ui_egui::image_to_midi::I2mRegionModel,
    /// 曲线弯折按钮（空 = 不显示）。
    bend_buttons: lumino_ui_egui::bend_buttons::BendButtonsModel,
    /// 文字工具覆盖层（None = 不显示）。
    text_tool: Option<lumino_ui_egui::text_tool::TextToolModel>,
    /// 力度 / CC / Tempo 面板骨架（None = 不可见）。
    velocity_panel: Option<lumino_ui_egui::velocity_panel::VelocityPanelModel>,
    mask_panel: Option<lumino_ui_egui::mask_panel::MaskPanelModel>,
    /// 纹理预览（渲染器 16:9 卡片 / 全屏瀑布流）；`Some` 时由 egui 铺图。
    video_preview: Option<lumino_ui_egui::chrome::TexturePreviewModel>,
    /// 素材删除确认（模态）；`Some` 时由 egui 绘制并接管输入。
    material_delete: Option<lumino_ui_egui::material_delete::MaterialDeleteModel>,
    /// 混音台（布局 + 模型）；`Some` 时由 egui 绘制并接管输入。
    mixer: Option<(
        lumino_ui_egui::mixer::MixerLayout,
        lumino_ui_egui::mixer::MixerModel,
    )>,
    /// 视频剪辑时间轴（真实矩形 + 模型）；`Some` 且开关打开时由 egui 绘制。
    video_timeline: Option<(
        egui::Rect,
        lumino_ui_egui::video_clip_timeline::VideoTimelineModel,
    )>,
    /// 本帧要提交给 egui 的指针 / 滚轮 / 修饰键 / 焦点事件。
    events: Vec<egui::Event>,
    /// 窗口是否持有焦点。
    focused: bool,
    /// 清屏色：`Some` 表示 egui 独占整窗（生产渲染路径），
    /// `None` 表示叠加在 旧版 `present` 之上（回退路径）。
    clear: Option<wgpu::Color>,
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
            clock: clock::FrameClock::new(),
            legacy_clock: std::env::var_os("LUMINO_EGUI_LEGACY_CLOCK").is_some_and(|v| v != "0"),
            clock_diag: std::env::var_os("LUMINO_CLOCK_DIAG").is_some(),
            frames: 0,
            blocked: Vec::new(),
        }
    }

    /// 渲染一帧外壳并提交到目标纹理。
    ///
    /// 返回值末尾的 `bool` 表示 egui 本帧是否需要键盘输入（文本编辑中），
    /// 调用方据此决定下一帧按键事件是否只给 egui。
    fn draw(&mut self, frame: OverlayFrame<'_>) -> OverlayOutput {
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

        // 渲染器预览 / 全屏瀑布流：同一张 `waterfall_player.view` 纹理。
        match &frame.render_preview {
            Some(view) => self.painter.set_external_texture(
                frame.device,
                lumino_ui_egui::chrome::RENDER_PREVIEW_TEXTURE_ID,
                view,
            ),
            None => self
                .painter
                .clear_external_texture(lumino_ui_egui::chrome::RENDER_PREVIEW_TEXTURE_ID),
        }

        // 编辑器画布：把 gfx 的离屏纹理注册成 egui 外部纹理，并由 egui 画成底层。
        match &frame.editor_texture {
            Some(view) => {
                self.painter.set_external_texture(
                    frame.device,
                    lumino_ui_egui::chrome::EDITOR_TEXTURE_ID,
                    view,
                );
                lumino_ui_egui::chrome::set_base_texture(
                    &self.ctx,
                    Some(lumino_ui_egui::chrome::EDITOR_TEXTURE_ID),
                );
            }
            None => {
                self.painter
                    .clear_external_texture(lumino_ui_egui::chrome::EDITOR_TEXTURE_ID);
                lumino_ui_egui::chrome::set_base_texture(&self.ctx, None);
            }
        }

        // 让 egui 以真实 DPI 布局与栅格化字体：否则字体图集按 1× 生成、
        // 再由着色器放大会发虚（几何位置不变，但字形明显更糊）。
        self.ctx.set_pixels_per_point(frame.pixels_per_point);

        let logical = [
            frame.size_px[0] as f32 / frame.pixels_per_point,
            frame.size_px[1] as f32 / frame.pixels_per_point,
        ];
        let mut input = clock::raw_input(
            frame.events,
            [logical[0], logical[1]],
            frame.focused,
            self.clock.elapsed(),
        );
        let input_time = input.time;
        if self.legacy_clock {
            // 诊断 A/B：退回「不填 time，egui 按 predicted_dt 推时钟」的旧行为。
            input.time = None;
        }

        let ctx = self.ctx.clone();
        let icons = &mut self.icons;
        // 真实窗口逻辑尺寸：路由栏底部按钮据此锚定（`ui.max_rect()` 可能无界）。
        lumino_ui_egui::chrome::set_window_size(
            &ctx,
            egui::vec2(
                frame.size_px[0] as f32 / frame.pixels_per_point.max(1e-3),
                frame.size_px[1] as f32 / frame.pixels_per_point.max(1e-3),
            ),
        );
        // 走带标尺：把生产模型交给 egui（`LUMINO_EGUI_ARRANGEMENT=1` 时才会真正绘制）。
        lumino_ui_egui::chrome::set_arrangement(&ctx, frame.arrangement.clone());
        lumino_ui_egui::chrome::set_video_timeline(&ctx, frame.video_timeline.clone());
        lumino_ui_egui::chrome::set_renderer_panel(&ctx, frame.renderer_panel.clone());
        lumino_ui_egui::chrome::set_mixer(&ctx, frame.mixer.clone());
        lumino_ui_egui::chrome::set_material_delete(&ctx, frame.material_delete.clone());
        lumino_ui_egui::chrome::set_video_preview(&ctx, frame.video_preview.clone());
        lumino_ui_egui::chrome::set_mask_panel(&ctx, frame.mask_panel.clone());
        // 右下角浮层：Toast 通知 + 首次使用帮助提示（旧版 视图已删除）。
        lumino_ui_egui::chrome::set_toast(&ctx, Some(frame.toast.clone()));
        lumino_ui_egui::chrome::set_help_tip(&ctx, frame.help_tip.clone());
        lumino_ui_egui::chrome::set_egg(&ctx, frame.egg);
        lumino_ui_egui::chrome::set_piano_roll_menu(&ctx, frame.piano_roll_menu);
        lumino_ui_egui::chrome::set_editor_buttons(&ctx, frame.editor_buttons.clone());
        lumino_ui_egui::chrome::set_marquee(&ctx, frame.marquee.clone());
        lumino_ui_egui::chrome::set_shape_preview(&ctx, frame.shape_preview.clone());
        lumino_ui_egui::chrome::set_curve_preview(&ctx, frame.curve_preview.clone());
        lumino_ui_egui::chrome::set_remote_cursors(&ctx, frame.remote_cursors.clone());
        lumino_ui_egui::chrome::set_editor_overlay(&ctx, frame.editor_overlay.clone());
        lumino_ui_egui::chrome::set_i2m_region(&ctx, frame.i2m_region);
        lumino_ui_egui::chrome::set_bend_buttons(&ctx, frame.bend_buttons.clone());
        lumino_ui_egui::chrome::set_text_tool(&ctx, frame.text_tool.clone());
        lumino_ui_egui::chrome::set_velocity_panel(&ctx, frame.velocity_panel.clone());

        let output = ctx.run_ui(input, |ui| {
            chrome::show_bands(ui, frame.model, icons);
            // 音频导出：整块面板覆盖右侧内容区。
            if let Some(panel) = &frame.audio_export {
                lumino_ui_egui::audio_export::show_panel(ui, panel);
            }
            // 形状抽屉：左侧面板 + 分割线 + 右侧画布全部由 egui 绘制。
            if let Some(panel) = &frame.shape_drawer {
                lumino_ui_egui::shape_drawer::show(ui, panel);
                lumino_ui_egui::shape_canvas::show_splitter(
                    ui,
                    egui::Rect::from_min_size(
                        egui::pos2(panel.rect.right(), panel.rect.top()),
                        egui::vec2(SPLITTER_W, panel.rect.height()),
                    ),
                    frame.palette.selected_bg,
                );
            }
            if let Some(canvas) = &frame.shape_canvas {
                lumino_ui_egui::shape_canvas::show(ui, canvas);
            }
            if let Some(panel) = &frame.video_export {
                lumino_ui_egui::video_export::show_panel(ui, panel);
            }
        });

        // `run_ui` 不返回闭包值：动作统一由 `OverlayActions::take` 取走，
        // 这里只要占用矩形与文本焦点。
        self.diagnose_clock(&ctx, input_time);
        let mut blocked = chrome::menu::take_blocked(&ctx);
        // 音频导出面板不透明覆盖右侧内容区：整块进入占用列表，点击不再落到 旧版。
        if let Some(panel) = &frame.audio_export {
            blocked.push(panel.rect);
        }
        // 形状抽屉：面板 + 分割线 + 画布整块接管。
        if let Some(panel) = &frame.shape_drawer {
            blocked.push(egui::Rect::from_min_max(
                panel.rect.min,
                egui::pos2(panel.rect.right() + SPLITTER_W, panel.rect.bottom()),
            ));
        }
        if let Some(canvas) = &frame.shape_canvas {
            blocked.push(canvas.rect);
        }
        if let Some(panel) = &frame.video_export {
            blocked.push(panel.rect);
        }

        let mut textures = output.textures_delta;
        let egui_cursor = output.platform_output.cursor_icon;
        let ime = output.platform_output.ime;
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
                clear: frame.clear,
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
        self.blocked = blocked;
        OverlayOutput {
            cursor: egui_cursor,
            ime,
        }
    }

    /// 指定点是否落在 egui 本帧占用区内。
    ///
    /// 占用区由各面板在绘制时登记（菜单 / 弹层 / 浮动面板 / 模态），
    /// 与 `layer_id_at` 的浮层命中互补：前者覆盖 root 层手工绘制的大面板，
    /// 后者覆盖 `Area` / `Window` 弹层。
    fn pointer_blocked(&self, pos: egui::Pos2) -> bool {
        self.blocked.iter().any(|rect| rect.contains(pos))
    }

    /// 时钟错位诊断（`LUMINO_CLOCK_DIAG=1`）。
    ///
    /// egui 的点击 / 长按 / 双击判定都读 `InputState::time`。若 `RawInput::time`
    /// 漏填，egui 会按 `predicted_dt` 每帧推进，虚拟时钟在高刷新率下比真实
    /// 时间快数倍，正常点击被判超时——真机表现就是「点不动」。
    ///
    /// 这里把 egui 内部时钟与宿主真实秒数对一次，偏差超过 0.25s 就打点，
    /// 便于在用户机器上直接取证（不设变量时零开销）。
    fn diagnose_clock(&self, ctx: &egui::Context, input_time: Option<f64>) {
        if !self.clock_diag {
            return;
        }
        let egui_time = ctx.input(|i| i.time);
        let Some(real) = input_time else {
            log::warn!("[CLOCK] RawInput::time 为空：egui 按帧推时钟，高刷新率下点击会失效");
            return;
        };
        let drift = egui_time - real;
        if drift.abs() > 0.25 {
            log::warn!(
                "[CLOCK] egui={egui_time:.3}s real={real:.3}s drift={drift:+.3}s（>0.25s 时点击判定会失真）"
            );
        }
    }
}

impl Host {
    /// 绘制 egui 外壳。
    ///
    /// `clear = Some(color)` 时 egui 独占整窗（生产渲染路径）；`None` 由调用方
    /// 保证底层已绘制（当前生产路径不使用）。
    pub(super) fn render_egui_overlay(
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
        // 画布几何：egui 是唯一布局来源（旧版 不再参与，取代 `Grid` program
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
        // 形状抽屉：面板 + 画布都由 egui 绘制（旧版 只保留标题栏）。
        let shape_panel_rect = (self.root.state.dialog_type
            == crate::state::root_state::DialogType::ShapeDrawer)
            .then(|| {
                let width = self.root.state.shape_drawer.panel_width;
                // 对话框窗口：`ChromeModel.titlebar` 为 None，但 旧版 仍画 30px 自制标题栏，
                // 因此这里按 `use_native_titlebar` 判断（与画布动作端的 rect 同源）。
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
        // 瀑布流面板可见且纹理已渲染时，把视图交给 egui 采样（旧版 侧同位置
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
            material_delete: self.root.material_delete_model(),
            video_preview: self.root.video_preview_model(window_area, content_area),
            mask_panel: self.root.mask_panel_model(),
            toast: self.root.egui_toast_model(),
            help_tip: self.root.egui_help_tip_model(),
            egg: self.root.egui_egg_model(),
            piano_roll_menu: self.root.piano_roll_context_menu_model(),
            editor_buttons: self.root.editor_buttons_model(),
            marquee: self.root.marquee_model(),
            shape_preview: self.root.shape_preview_model(),
            curve_preview: self.root.curve_preview_model(),
            remote_cursors: self.root.remote_cursors_model(),
            editor_overlay: self.root.editor_overlay_model(),
            i2m_region: self.root.i2m_region_model(),
            bend_buttons: self.root.bend_buttons_model(),
            text_tool: self.root.text_tool_model(),
            velocity_panel: self.root.velocity_panel_model(),
            render_preview: self.root.waterfall_player.view.clone(),
            mixer: self.root.mixer_model(
                size.width as f32 / pixels_per_point.max(1e-3),
                size.height as f32 / pixels_per_point.max(1e-3),
            ),
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
        let panel_blocked = self
            .render_ctx
            .egui_overlay
            .as_ref()
            .is_some_and(|overlay| overlay.pointer_blocked(pos));
        panel_blocked || self.egui_layer_owns_pos(pos)
    }
}

/// 形状抽屉面板与画布之间的分割线宽度（与生产 旧版 视图一致）。
pub(crate) const SPLITTER_W: f32 = 6.0;

/// egui 光标矩形（逻辑点）→ winit IME 锚点（物理像素）。
///
/// 候选窗尺寸至少 1px：零尺寸在部分 IME 下会被忽略，导致候选窗跑到屏幕角落。
fn ime_cursor_area(
    rect: egui::Rect,
    pixels_per_point: f32,
) -> (
    winit::dpi::PhysicalPosition<i32>,
    winit::dpi::PhysicalSize<u32>,
) {
    let scale = if pixels_per_point > 0.0 {
        pixels_per_point
    } else {
        1.0
    };
    let min = rect.min * scale;
    let size = rect.size() * scale;
    (
        winit::dpi::PhysicalPosition::new(min.x.round() as i32, min.y.round() as i32),
        winit::dpi::PhysicalSize::new(
            size.x.max(1.0).round() as u32,
            size.y.max(1.0).round() as u32,
        ),
    )
}

/// 把 egui 的 IME 锚点应用到窗口；`ime_allowed` 是宿主的缓存状态。
///
/// 独立成纯函数便于无窗口测试：只依赖 [`crate::host::HostWindow`] 抽象。
fn apply_ime_to_window(
    window: &dyn crate::host::HostWindow,
    ime_allowed: &mut bool,
    ime: Option<egui::output::IMEOutput>,
    pixels_per_point: f32,
) {
    let allowed = ime.is_some();
    if *ime_allowed != allowed {
        window.set_ime_allowed(allowed);
        *ime_allowed = allowed;
        log::debug!("[IME] set_ime_allowed({allowed})");
    }
    let Some(ime) = ime else {
        return;
    };
    let (position, size) = ime_cursor_area(ime.cursor_rect, pixels_per_point);
    window.set_ime_cursor_area(position, size);
}

/// 指针事件的坐标除数。
///
/// 布局与绘制都以 `pixels_per_point` 为准；指针必须用**同一个**值换算到点空间。
/// winit 的 `CursorMoved.position` 是**物理像素**，egui 用逻辑点：只除一次 ppp。
///
/// 此前这里多除了一次（ppp²），指针坐标比绘制位置小 1.25 倍。真机取证：
/// 客户端逻辑 (24,240) 在 egui 里变成 (19.2,192)，于是窗口下方约 20% 的区域
/// 永远命中不到——混音台与路由栏底部按钮「点了没反应」都是这个原因。
pub(crate) fn input_scale(ppp: f32) -> f32 {
    if ppp > 0.0 { ppp } else { 1.0 }
}

#[cfg(test)]
mod tests {
    use egui::{Area, Context, Id, Order, Rect, pos2, vec2};

    /// 输入归属依赖的 egui 事实：浮层 `Area` 在 `layer_id_at` 里返回非
    /// `Background` 图层，而 root Ui / 画布所在层是 `Background`。
    ///
    /// 这条假设一旦被 egui 升级打破，宿主就会重新把弹层点击漏给画布——
    /// 用测试钉死，不靠记忆。
    #[test]
    fn layer_id_at_separates_foreground_areas_from_background() {
        let ctx = Context::default();
        // `Area` 首帧是 sizing pass（`interactable` 被临时置 false），必须跑两帧
        // 才能进入命中栈——生产侧 `Area::show` 会主动 `request_repaint`。
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                ui.label("background");
                Area::new(Id::unique("test-foreground"))
                    .order(Order::Foreground)
                    .fixed_pos(pos2(100.0, 100.0))
                    .show(ui.ctx(), |ui| {
                        ui.set_min_size(vec2(80.0, 40.0));
                        ui.label("panel");
                    });
            });
            // `TexturesDelta` 必须消费掉，否则 drop 时 epaint 会 panic。
            output.textures_delta.clear();
        }

        let inside = ctx.layer_id_at(pos2(140.0, 120.0)).expect("浮层应存在");
        assert_ne!(
            inside.order,
            Order::Background,
            "浮层命中必须是非 Background 图层"
        );

        let outside = ctx.layer_id_at(pos2(10.0, 10.0)).expect("root 层应存在");
        assert_eq!(
            outside.order,
            Order::Background,
            "画布/root 层命中必须是 Background"
        );
    }

    /// IME 候选窗锚点必须按 DPI 换算到物理像素，且尺寸不小于 1px。
    #[test]
    fn ime_cursor_area_scales_to_physical_pixels() {
        let rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(2.0, 18.0));
        let (position, size) = super::ime_cursor_area(rect, 1.25);
        assert_eq!((position.x, position.y), (13, 25));
        assert_eq!((size.width, size.height), (3, 23));

        let (position, size) = super::ime_cursor_area(rect, 0.0);
        assert_eq!((position.x, position.y), (10, 20), "非法 ppp 回退 1.0");
        assert_eq!((size.width, size.height), (2, 18));
    }

    /// egui 文本编辑开始 / 结束时，宿主必须开关系统输入法并锚定候选窗。
    #[test]
    fn ime_output_toggles_window_state() {
        use crate::host::HostWindow;
        use std::sync::{
            Mutex,
            atomic::{AtomicBool, Ordering},
        };

        #[derive(Default)]
        struct Mock {
            allowed: AtomicBool,
            area: Mutex<Option<(i32, i32, u32, u32)>>,
        }

        impl HostWindow for Mock {
            fn request_redraw(&self) {}
            fn set_cursor(&self, _icon: winit::window::CursorIcon) {}
            fn set_cursor_visible(&self, _visible: bool) {}
            fn scale_factor(&self) -> f64 {
                1.0
            }
            fn is_maximized(&self) -> bool {
                false
            }
            fn set_ime_allowed(&self, allowed: bool) {
                self.allowed.store(allowed, Ordering::SeqCst);
            }
            fn set_ime_cursor_area(
                &self,
                position: winit::dpi::PhysicalPosition<i32>,
                size: winit::dpi::PhysicalSize<u32>,
            ) {
                *self.area.lock().expect("IME 区域锁不得中毒") =
                    Some((position.x, position.y, size.width, size.height));
            }
        }

        let mock = Mock::default();
        let mut allowed = false;
        let ime = egui::output::IMEOutput {
            purpose: egui::IMEPurpose::Normal,
            rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 20.0)),
            cursor_rect: Rect::from_min_size(pos2(10.0, 20.0), vec2(2.0, 18.0)),
            should_interrupt_composition: false,
        };

        super::apply_ime_to_window(&mock, &mut allowed, Some(ime), 1.25);
        assert!(allowed, "文本编辑开始必须开启输入法");
        assert!(mock.allowed.load(Ordering::SeqCst));
        assert_eq!(
            *mock.area.lock().expect("IME 区域锁不得中毒"),
            Some((13, 25, 3, 23)),
            "候选窗必须按 DPI 锚到光标处"
        );

        super::apply_ime_to_window(&mock, &mut allowed, None, 1.25);
        assert!(!allowed, "文本编辑结束必须关闭输入法");
        assert!(!mock.allowed.load(Ordering::SeqCst));
    }
}
