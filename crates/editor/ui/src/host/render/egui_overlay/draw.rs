//! `EguiOverlay` 的创建、逐帧绘制、面板阻挡区与时钟诊断。
//!
//! 这里是 egui 全窗口绘制的唯一入口；绘制顺序（背景 → 面板 → 浮层）不可调整。

use super::*;

impl EguiOverlay {
    /// 用当前 surface 格式创建叠加层。
    pub(super) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        tracing::info!("egui 外壳叠加层已启用：目标格式 {format:?}");
        let ctx = egui::Context::default();
        lumino_ui_egui::theme::apply(&ctx);
        lumino_ui_egui::fonts::install(&ctx);
        Self {
            ctx,
            icons: IconCache::default(),
            pixels_per_point: 0.0,
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
    pub(super) fn draw(&mut self, frame: OverlayFrame<'_>) -> OverlayOutput {
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
        // 只在 DPI 真正变化时重设：逐帧写入会反复重建字体栅格。
        if (self.pixels_per_point - frame.pixels_per_point).abs() > f32::EPSILON {
            self.ctx.set_pixels_per_point(frame.pixels_per_point);
            self.pixels_per_point = frame.pixels_per_point;
        }

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
        // 「无面板」默认第一屏：主区域完全没有视图时显示品牌图标 + 字标；
        // 尚未载入真实工程时右栏追加操作按钮（新建 / 打开 / 设置 / 协作）。
        lumino_ui_egui::chrome::set_first_screen(&ctx, frame.first_screen.clone());
        // 走带标尺：把生产模型交给 egui（`LUMINO_EGUI_ARRANGEMENT=1` 时才会真正绘制）。
        lumino_ui_egui::chrome::set_arrangement(&ctx, frame.arrangement.clone());
        lumino_ui_egui::chrome::set_video_timeline(&ctx, frame.video_timeline.clone());
        lumino_ui_egui::chrome::set_renderer_panel(&ctx, frame.renderer_panel.clone());
        lumino_ui_egui::chrome::set_video_preview(&ctx, frame.video_preview.clone());
        lumino_ui_egui::chrome::set_mask_panel(&ctx, frame.mask_panel.clone());
        // 右下角浮层：Toast 通知 + 首次使用帮助提示（上一代 视图已删除）。
        lumino_ui_egui::chrome::set_toast(&ctx, Some(frame.toast.clone()));
        lumino_ui_egui::chrome::set_help_tip(&ctx, frame.help_tip.clone());
        lumino_ui_egui::chrome::set_egg(&ctx, frame.egg);
        lumino_ui_egui::chrome::set_piano_roll_menu(&ctx, frame.piano_roll_menu);
        lumino_ui_egui::chrome::set_editor_buttons(&ctx, frame.editor_buttons.clone());
        lumino_ui_egui::chrome::set_marquee(&ctx, frame.marquee.clone());
        lumino_ui_egui::chrome::set_shape_edit(&ctx, frame.shape_edit.clone());
        lumino_ui_egui::chrome::set_shape_list(&ctx, frame.shape_list.clone());
        lumino_ui_egui::chrome::set_area_palette(&ctx, frame.area_palette.clone());
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
        // 音频导出面板不透明覆盖右侧内容区：整块进入占用列表，点击不再落到 上一代。
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
    pub(super) fn pointer_blocked(&self, pos: egui::Pos2) -> bool {
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
