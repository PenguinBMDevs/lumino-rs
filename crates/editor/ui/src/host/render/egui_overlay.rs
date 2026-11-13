//! 实验性 egui 外壳叠加层（`LUMINO_EGUI_OVERLAY=1` 时启用）。
//!
//! 目的：在真实窗口 surface / 真实 `Device` 上验证 `lumino-egui-bridge`，
//! 并把生产状态（标题栏 / 状态栏）逐步搬到 egui。默认关闭，不改变现有画面。
//!
//! 当前接管范围：
//! - 标题栏 30px：底色随窗口聚焦变化、真实菜单标签（按语言）、logo、窗口按钮；
//! - 状态栏 20px：左侧提示文本（悬停说明 / 音符数 / 就绪）、CPU / MEM / FPS、右侧文本。
//!
//! 只绘制这些 band，其余区域保持 旧版 画面；band 用不透明底色完全覆盖同位置
//! 的 旧版 版本，因此不会出现文字重影。
//!
//! # 输入
//!
//! 指针移动 / 滚轮 / 修饰键 / 焦点同步给 egui（悬停高亮、`ScrollArea` 滚动、
//! 菜单点击都要用到）。鼠标按键只在**菜单占用区**（标题栏菜单标签、已展开的
//! 下拉/子面板）内消费；其余区域仍把事件交给 旧版，动作由 旧版 执行。

use egui::{Rect, Vec2};
use lumino_egui_bridge::{Painter, Screen};
use lumino_ui_egui::chrome::{self, ChromeModel};
use lumino_ui_egui::icons::IconCache;
use wgpu;
use winit;

use crate::host::Host;

mod actions;
mod canvas;
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
            frames: 0,
        }
    }

    /// 渲染一帧外壳并提交到目标纹理。
    ///
    /// 返回值末尾的 `bool` 表示 egui 本帧是否需要键盘输入（文本编辑中），
    /// 调用方据此决定下一帧按键事件是否只给 egui。
    fn draw(&mut self, frame: OverlayFrame<'_>) -> (Vec<Rect>, bool, egui::CursorIcon) {
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
        (blocked, self.ctx.text_edit_focused(), egui_cursor)
    }
}

impl Host {
    /// 绘制 egui 外壳。
    ///
    /// `clear = Some(color)` 时 egui 独占整窗（先清屏再画，生产渲染路径）；
    /// `None` 时叠加在 旧版 `present` 之上（`LUMINO_EGUI_ONLY=0` 回退路径）。
    pub(super) fn render_egui_overlay(
        &mut self,
        frame: &wgpu::SurfaceTexture,
        view: &wgpu::TextureView,
        clear: Option<wgpu::Color>,
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
        let events = self.egui_input.take();
        let focused = self.egui_input.focused();
        // Device/Queue 是 Arc 句柄，克隆避免与 `egui_overlay` 的可变借用冲突。
        let device = self.render_ctx.device.clone();
        let queue = self.render_ctx.queue.clone();

        let overlay = self
            .render_ctx
            .egui_overlay
            .get_or_insert_with(|| EguiOverlay::new(&device, format));
        let (blocked, wants_keyboard, egui_cursor) = overlay.draw(OverlayFrame {
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
        // 下一帧点击命中这些矩形时，事件只给 egui，不再进 旧版 队列。
        self.egui_blocked = blocked;
        self.egui_input.set_wants_keyboard(wants_keyboard);
        self.apply_egui_cursor(egui_cursor);
        self.apply_overlay_actions(actions, logical_width);
    }

    /// 把 winit 指针 / 滚轮 / 修饰键 / 焦点事件转成 egui 事件。
    ///
    /// 返回 `true` 表示该事件落在 egui 的菜单占用区（标题栏/下拉面板）内，
    /// 调用方必须**不**再把它交给 旧版——否则会同时触发下层工具栏/侧栏的点击。
    /// 其余区域保持「只观察不消费」，动作仍由 旧版 执行。
    pub(crate) fn capture_egui_overlay_input(&mut self, event: &winit::event::WindowEvent) -> bool {
        use winit::event::{MouseScrollDelta, WindowEvent};

        if !overlay_enabled() {
            return false;
        }
        // 只有**会改变 egui 输入状态**的事件才需要驱动一帧。
        //
        // 这里曾经无条件 `request_redraw()`：`RedrawRequested` 自己也会再次请求，
        // 形成永不停歇的「渲染 → 请求重绘 → 渲染」自旋（Mailbox present mode
        // 下没有 vsync 上限，实测空闲也能跑满；debug 构建因为每帧慢，反而不明显）。
        // 后果不是画面错，而是事件循环永远进不了 `ControlFlow::Wait`、鼠标消息
        // 被 WM_PAINT 饿死——真机表现就是「点击要连点很多次才响应」。
        if input_requests_repaint(event) {
            self.window_ctx.window.request_redraw();
        }
        // 与渲染用的 `pixels_per_point` 同源：两者必须一致，否则命中坐标
        // 与实际绘制的控件位置会按比例错开（真机实测：指针比布局大 1.25 倍时，
        // 点击路由栏会整体下移一格）。
        let scale = input_scale(self.render_ctx.viewport.scale_factor());
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
                // 首击兜底：`pointer` 只由 `CursorMoved` 写入，启动后 / `CursorLeft`
                // 后 / 自动化注入的第一次按键会因 `pointer == None` 被静默丢弃。
                // 用宿主已记录的逻辑光标位置补一次，保证 egui 一定收到这次按下。
                if self.egui_input.pointer.is_none()
                    && let Some(pos) = self.window_ctx.cursor_position
                {
                    self.egui_input.pointer_moved(egui::pos2(pos.x, pos.y));
                }
                self.egui_input
                    .button(button, *state == winit::event::ElementState::Pressed);
                // 设置 / 保存确认窗口由 egui 整窗绘制：按键只给 egui，避免同一击
                // 同时触发下层 旧版 控件（例如云管理「添加连接」打开两个窗口）。
                // 其余对话框仍依赖 旧版 let-through，不消费。
                self.dialog_input_exclusive() || self.egui_owns_pointer()
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
                // 只有 egui 正在编辑文本时才消费：否则快捷键仍要落到 旧版。
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
    /// 这类窗口必须消费鼠标按键，不能再落到 旧版。
    fn dialog_input_exclusive(&self) -> bool {
        if !self.root.state.is_dialog_window {
            return false;
        }
        match self.root.state.dialog_type {
            crate::state::root_state::DialogType::Settings
            | crate::state::root_state::DialogType::SaveConfirm => true,
            // 工程设置 / 撤销历史 / 批量编辑的顶部 30px 自制标题栏（红绿灯 / 拖动）
            // 仍由 旧版 绘制：该区域内的按键不消费，其余内容区只给 egui。
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
            | crate::state::root_state::DialogType::RecoverTrack
            | crate::state::root_state::DialogType::Collaboration
            | crate::state::root_state::DialogType::CloudBrowser
            | crate::state::root_state::DialogType::CloudNotice => {
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
    pub(crate) fn egui_pointer_blocked(&self) -> bool {
        self.egui_input
            .pointer
            .is_some_and(|pos| self.egui_blocked.iter().any(|rect| rect.contains(pos)))
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

    /// 画布输入是否应让位给 egui（手工占用区 ∪ 浮层图层）。
    ///
    /// 指针优先取 egui 事件缓冲里的位置；触摸路径没有 `CursorMoved`，
    /// 回退到宿主记录的逻辑光标位置，保证触摸点按也能正确让位。
    pub(crate) fn egui_owns_pointer(&self) -> bool {
        let pos = self.egui_input.pointer.or_else(|| {
            self.window_ctx
                .cursor_position
                .map(|p| egui::pos2(p.x, p.y))
        });
        let Some(pos) = pos else {
            return false;
        };
        self.egui_blocked.iter().any(|rect| rect.contains(pos)) || self.egui_layer_owns_pos(pos)
    }
}

/// 形状抽屉面板与画布之间的分割线宽度（与生产 旧版 视图一致）。
pub(crate) const SPLITTER_W: f32 = 6.0;

/// 指针事件的坐标除数。
///
/// 布局与绘制都以 `pixels_per_point` 为准；指针必须用**同一个**值换算到点空间。
/// `LUMINO_EGUI_INPUT_SCALE` 可强制覆盖，用于真机核对（不改代码即可验证方向）。
fn input_scale(ppp: f32) -> f32 {
    static OVERRIDE: std::sync::OnceLock<Option<f32>> = std::sync::OnceLock::new();
    if let Some(value) = *OVERRIDE.get_or_init(|| {
        std::env::var("LUMINO_EGUI_INPUT_SCALE")
            .ok()
            .and_then(|raw| raw.parse::<f32>().ok())
            .filter(|value| *value > 0.0)
    }) {
        return value;
    }
    let ppp = if ppp > 0.0 { ppp } else { 1.0 };
    // winit 的 `CursorMoved.position` 是**物理像素**，egui 用逻辑点：只除一次 ppp。
    //
    // 此前这里多除了一次（ppp²），指针坐标比绘制位置小 1.25 倍。真机取证：
    // 客户端逻辑 (24,240) 在 egui 里变成 (19.2,192)，于是窗口下方约 20% 的区域
    // 永远命中不到——混音台与路由栏底部按钮「点了没反应」都是这个原因。
    // `LUMINO_EGUI_INPUT_SCALE` 仍可强制覆盖用于排查。
    ppp
}

/// 该窗口事件是否必须驱动一帧 egui。
///
/// **绝不能**把 `RedrawRequested` 算进来：那会让 `RedrawRequested` 的处理
/// 再排一帧，形成无上限的「渲染 → 请求重绘 → 渲染」自旋。`Mailbox`
/// present mode 没有 vsync 上限，事件循环因此永远进不了 `ControlFlow::Wait`，
/// 鼠标消息被 `WM_PAINT` 饿死——真机表现就是「点击要连点很多次才响应」。
///
/// 用纯函数 + 单测钉死：以后再加事件分支时，别把非输入事件放进来。
fn input_requests_repaint(event: &winit::event::WindowEvent) -> bool {
    use winit::event::WindowEvent;
    matches!(
        event,
        WindowEvent::CursorMoved { .. }
            | WindowEvent::CursorLeft { .. }
            | WindowEvent::MouseWheel { .. }
            | WindowEvent::MouseInput { .. }
            | WindowEvent::ModifiersChanged(_)
            | WindowEvent::KeyboardInput { .. }
            | WindowEvent::Ime(_)
            | WindowEvent::Focused(_)
    )
}

/// egui 叠加层是否启用。
///
/// **默认开启**：外壳 band / 对话框 / 面板 / 编辑器画布底层均已迁到 egui，
/// 旧版 侧只在下方保留同位置内容（已被不透明覆盖），后续逐步删除。
/// `LUMINO_EGUI_OVERLAY=0` 可回退到纯 旧版 通道，用于对照排查。
pub(crate) fn overlay_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("LUMINO_EGUI_OVERLAY")
            .map(|value| value != "0")
            .unwrap_or(true)
    })
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

    /// 重绘自旋回归：`RedrawRequested` 不能再驱动下一帧。
    ///
    /// 曾经的实现对所有窗口事件无条件 `request_redraw()`，`RedrawRequested`
    /// 自己也会再次入列 → 无上限自旋（Mailbox 下没有 vsync 上限）。事件循环
    /// 永远进不了 `ControlFlow::Wait`，鼠标消息被 `WM_PAINT` 饿死，真机表现
    /// 为「点击要连点很多次才响应」。fast-release 比 debug 明显，因为每帧更快。
    #[test]
    fn only_input_events_request_repaint() {
        use winit::event::WindowEvent;

        assert!(
            !super::input_requests_repaint(&WindowEvent::RedrawRequested),
            "RedrawRequested 绝不能再次请求重绘（会形成自旋）"
        );
        assert!(
            !super::input_requests_repaint(&WindowEvent::Resized(winit::dpi::PhysicalSize::new(
                1280, 720
            ))),
            "尺寸变化由系统重绘驱动，不需要再排一帧"
        );
        assert!(
            super::input_requests_repaint(&WindowEvent::Focused(true)),
            "焦点变化要刷新 egui 的 focused 状态"
        );
        assert!(
            super::input_requests_repaint(&WindowEvent::ModifiersChanged(
                winit::event::Modifiers::default()
            )),
            "修饰键变化要同步给 egui（滚轮/快捷键事件需要随附）"
        );
    }
}
