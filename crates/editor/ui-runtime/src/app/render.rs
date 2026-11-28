//! 原生运行时渲染：音符上传、渲染参数、画布纹理、egui 外壳与提交。
//!
//! `render` 只保留帧次序；每一段都能单独读：上传 → 参数 → 纹理 → 外壳模型 →
//! `run_ui` → 占用区快照 → `present`。

use egui::{Pos2, RawInput, Rect, Vec2};
use lumino_ui_egui::chrome::{self, ChromeModel, StatusbarModel, TitlebarModel, ToolbarModel};
use winit::dpi::PhysicalSize;

use super::state::{NativeApp, note_border_width};

impl NativeApp {
    pub(super) fn render(&mut self) {
        let Some(window) = self.window.clone() else {
            return;
        };
        if self.gpu.is_none() {
            return;
        }
        let size = window.inner_size();
        let ppp = window.scale_factor() as f32;
        let logical = Vec2::new(size.width as f32 / ppp, size.height as f32 / ppp);

        self.upload_notes_if_needed();
        let params = self.render_params(size, ppp, logical);
        self.update_canvas_texture(params);
        self.publish_chrome_models();

        let raw = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, logical)),
            events: std::mem::take(&mut self.events),
            time: self.start.map(|start| start.elapsed().as_secs_f64()),
            ..Default::default()
        };
        let output = self.run_ui(raw, logical);

        // 取走本帧占用区（菜单 / 弹层 / 面板）：下一次鼠标事件据此让位。
        // 与生产 `EguiOverlay` 同口径；`Area` 首帧 sizing pass 靠它兜底。
        self.blocked = chrome::menu::take_blocked(&self.ctx);

        if let Some(gpu) = self.gpu.as_mut() {
            gpu.present(&self.ctx, output);
        }
    }

    /// 编辑器音符 → gfx 渲染线程（`track_notes_gen` 变化时全量上传）。
    fn upload_notes_if_needed(&mut self) {
        let (Some(root), Some(canvas)) = (self.root.as_ref(), self.canvas.as_ref()) else {
            return;
        };
        let generation = root.editor.editor_state.data.track_notes_gen;
        if self.note_gen == Some(generation) {
            return;
        }
        let data = &root.editor.editor_state.data;
        let document = data.document.as_ref();
        let track_count = document.map_or(0, |doc| doc.track_count());
        let tracks: Vec<(usize, Vec<lumino_gfx::NoteInstance>)> = document
            .map(|doc| {
                (0..track_count)
                    .map(|track_id| {
                        // 高 16 位 = track_enc（track_id + 1）：cull shader 据此
                        // 判断「主音轨 / 洋葱皮可见区间」，低 16 位是描边像素。
                        // 只写描边宽度（track_enc = 0）会让所有音符被裁掉。
                        let border_width = note_border_width(track_id);
                        let instances = doc
                            .track_notes(track_id)
                            .iter()
                            .map(|note| {
                                lumino_gfx::NoteInstance::new(
                                    note.start_tick as f32,
                                    note.key,
                                    (note.end_tick.saturating_sub(note.start_tick)) as f32,
                                    [0.30, 0.62, 0.95, 1.0],
                                    border_width,
                                )
                            })
                            .collect();
                        (track_id, instances)
                    })
                    .collect()
            })
            .unwrap_or_default();
        canvas.upload_notes(tracks, track_count, data.current_track);
        self.note_gen = Some(generation);
    }

    /// 渲染参数必须与编辑器视图同源（scroll / zoom / 画布原点 / 键数 / PPQ），
    /// 否则音符与点击位置不一致，甚至被 canvas scissor 裁掉。
    fn render_params(
        &self,
        size: PhysicalSize<u32>,
        ppp: f32,
        logical: Vec2,
    ) -> lumino_gfx::RenderParams {
        self.root
            .as_ref()
            .map(|root| {
                let es = &root.editor.editor_state;
                let view = &es.view;
                lumino_gfx::RenderParams {
                    viewport_size: (size.width.max(1), size.height.max(1)),
                    logical_size: (logical.x, logical.y),
                    scale_factor: ppp,
                    scroll: (view.scroll_x, view.scroll_y),
                    zoom: (view.zoom_x, view.zoom_y),
                    keyboard_width: view.keyboard_width,
                    ruler_height: view.ruler_height,
                    canvas_offset: (es.canvas.offset_x, es.canvas.offset_y),
                    canvas_size: (es.canvas.size_x, es.canvas.size_y),
                    ppq: view.ppq as f32,
                    max_key_index: view.visible_key_count.saturating_sub(1) as f32,
                    time_signatures: es.data.time_signatures.clone(),
                    // 与 egui 深色底色区分，便于真机确认底层纹理确实来自 gfx。
                    background_color: [0.07, 0.10, 0.16, 1.0],
                    color_bg: [0.07, 0.10, 0.16, 1.0],
                    content_dirty: true,
                    ..Default::default()
                }
            })
            .unwrap_or_else(|| lumino_gfx::RenderParams {
                viewport_size: (size.width.max(1), size.height.max(1)),
                logical_size: (logical.x, logical.y),
                scale_factor: ppp,
                canvas_size: (logical.x, logical.y),
                background_color: [0.07, 0.10, 0.16, 1.0],
                color_bg: [0.07, 0.10, 0.16, 1.0],
                content_dirty: true,
                ..Default::default()
            })
    }

    /// gfx 编辑器画布 → egui 底层纹理（分离渲染线程）。
    fn update_canvas_texture(&mut self, params: lumino_gfx::RenderParams) {
        let Some(canvas) = self.canvas.as_ref() else {
            return;
        };
        let Some(gpu) = self.gpu.as_mut() else {
            return;
        };
        if let Some(view) = canvas.render(params) {
            gpu.painter
                .set_external_texture(&gpu.device, chrome::EDITOR_TEXTURE_ID, &view);
            chrome::set_base_texture(&self.ctx, Some(chrome::EDITOR_TEXTURE_ID));
        } else {
            gpu.painter
                .clear_external_texture(chrome::EDITOR_TEXTURE_ID);
            chrome::set_base_texture(&self.ctx, None);
        }
    }

    /// 编辑器悬浮层模型（形状 / I2M / 曲线 / 画刷等）→ egui 通道。
    fn publish_chrome_models(&self) {
        let root = self.root.as_ref();
        chrome::set_editor_buttons(
            &self.ctx,
            root.map(|root| root.editor_buttons_model())
                .unwrap_or_default(),
        );
        chrome::set_marquee(
            &self.ctx,
            root.map(|root| root.marquee_model()).unwrap_or_default(),
        );
        chrome::set_shape_edit(
            &self.ctx,
            root.map(|root| root.shape_edit_model()).unwrap_or_default(),
        );
        chrome::set_shape_list(
            &self.ctx,
            root.map(|root| root.shape_list_model()).unwrap_or_default(),
        );
        chrome::set_area_palette(
            &self.ctx,
            root.map(|root| root.area_palette_model())
                .unwrap_or_default(),
        );
        chrome::set_shape_preview(
            &self.ctx,
            root.map(|root| root.shape_preview_model())
                .unwrap_or_default(),
        );
        chrome::set_curve_preview(
            &self.ctx,
            root.map(|root| root.curve_preview_model())
                .unwrap_or_default(),
        );
        chrome::set_i2m_region(
            &self.ctx,
            root.map(|root| root.i2m_region_model()).unwrap_or_default(),
        );
        chrome::set_bend_buttons(
            &self.ctx,
            root.map(|root| root.bend_buttons_model())
                .unwrap_or_default(),
        );
        chrome::set_text_tool(&self.ctx, root.and_then(|root| root.text_tool_model()));
        chrome::set_velocity_panel(&self.ctx, root.and_then(|root| root.velocity_panel_model()));
    }

    /// 跑一帧 egui 外壳：生产 bands + 动作通道 + 探针控件。
    fn run_ui(&mut self, raw: RawInput, logical: Vec2) -> egui::FullOutput {
        let mut root = self.root.take();
        let model = root
            .as_ref()
            .map(|root| root.egui_chrome_model_native(logical.x))
            .unwrap_or_else(|| demo_model(0));
        let notes = root
            .as_ref()
            .map(|root| root.editor.editor_state.data.current_track_note_count())
            .unwrap_or(0);
        let clicks = &mut self.clicks;
        let mut icons = std::mem::take(&mut self.icons);
        let output = self.ctx.run_ui(raw, |ui| {
            // 直接跑生产 egui 外壳（标题栏 / 工具栏 / 左右栏 / 状态栏）。
            chrome::show_bands(ui, &model, &mut icons);
            // 菜单 / 工具栏 / 左栏 / 状态栏 / 标题栏动作 → 生产 `Message`。
            // 通道表与生产 Host 同源（`OverlayActions::take`），不再维护第二份清单。
            if let Some(root) = root.as_mut() {
                let actions = lumino_ui::host::OverlayActions::take(ui.ctx());
                let applied = root.apply_egui_actions(actions, logical);
                if applied > 0 {
                    tracing::debug!("原生运行时: 已应用 {} 条 egui 动作", applied);
                }
            }

            // 探针控件：验证输入仍能到达 egui。
            egui::Area::new(egui::Id::unique("lumino-runtime-probe"))
                .fixed_pos(egui::pos2(24.0, 96.0))
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.label("native runtime probe");
                        if ui.button("点我 +1").clicked() {
                            *clicks += 1;
                        }
                        ui.label(format!("clicks {clicks}"));
                        ui.label(format!("notes {notes}"));
                    });
                });
        });
        self.root = root;
        self.icons = icons;
        output
    }
}

/// 探针用的最小外壳模型：标题栏 + 默认工具栏 + 状态栏。
fn demo_model(clicks: u32) -> ChromeModel {
    ChromeModel {
        titlebar: Some(TitlebarModel {
            focused: true,
            menus: vec![
                "文件".to_owned(),
                "编辑".to_owned(),
                "视图".to_owned(),
                "帮助".to_owned(),
            ],
            menu: None,
            menu_only: false,
        }),
        toolbar: Some(ToolbarModel::default()),
        statusbar: StatusbarModel {
            left: format!("Lumino egui native runtime · clicks {clicks}"),
            left_accent: true,
            cpu_percent: None,
            memory_mb: 0.0,
            fps: None,
            right: String::new(),
            gpu: None,
            // 探针外壳不显示指标组（`fps: None`），MEM 说明保持为空即可。
            memory_tip: Default::default(),
        },
        ..ChromeModel::default()
    }
}
