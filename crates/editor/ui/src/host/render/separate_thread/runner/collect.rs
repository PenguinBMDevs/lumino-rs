//! 渲染数据收集 — 收集走带实例与渲染所需数据
//!
//! 包含 Host 的以下方法：
//! - `collect_render_data`: 收集渲染所需数据
//! - `collect_arrangement_instances`: 收集走带视图实例

use crate::host::Host;
use crate::host::render::data::RenderData;
use lumino_gfx::{ArrangementNoteInstance, ArrangementSceneParams, ArrangementViewColors};

impl Host {
    /// 收集走带视图全部实例（背景 + lane + 网格线 + 音符 + 演奏指示线）
    /// 屏幕坐标，每帧重建，二分查找加速 MidiDocument 音符读取
    pub(super) fn collect_arrangement_instances(&self) -> Vec<ArrangementNoteInstance> {
        puffin::profile_scope!("collect_arrangement_instances");

        let track_order: Vec<usize> = self.root.sidebar.tracks.iter().map(|t| t.id).collect();
        let viewport_info = self.collect_viewport_info();
        let av = &self.root.arrangement_view.viewport;

        let viewport = lumino_gfx::ArrangementViewport {
            scroll_x: av.scroll_x,
            scroll_y: av.scroll_y,
            zoom_x: av.zoom_x,
            zoom_y: av.zoom_y,
            track_height: av.track_height,
            canvas_offset: [viewport_info.canvas_offset.x, viewport_info.canvas_offset.y],
            canvas_size: [viewport_info.canvas_size.x, viewport_info.canvas_size.y],
            total_ticks: av.total_ticks,
            ppq: av.ppq,
        };

        let track_visible: Vec<bool> = self
            .root
            .sidebar
            .tracks
            .iter()
            .map(|t| !t.is_muted)
            .collect();

        // 从主题提取走带视图颜色
        let theme = self.root.theme();
        use crate::editor::grid::theme::ThemeExt;
        let is_light = theme.is_light();
        let palette = theme.extended_palette().background;
        let arr_bg = if is_light {
            palette.weak.color
        } else {
            palette.base.color
        };
        let arr_lane_even = if is_light {
            palette.weakest.color
        } else {
            palette.weak.color
        };
        let arr_lane_odd = if is_light {
            palette.strong.color
        } else {
            palette.base.color
        };
        let arr_measure_line = if is_light {
            iced_core::Color {
                a: 0.8,
                ..iced_core::Color::BLACK
            }
        } else {
            iced_core::Color {
                a: 0.8,
                ..iced_core::Color::WHITE
            }
        };
        let colors = ArrangementViewColors {
            bg: [arr_bg.r, arr_bg.g, arr_bg.b],
            lane_even: [arr_lane_even.r, arr_lane_even.g, arr_lane_even.b],
            lane_odd: [arr_lane_odd.r, arr_lane_odd.g, arr_lane_odd.b],
            measure_line: [
                arr_measure_line.r,
                arr_measure_line.g,
                arr_measure_line.b,
                arr_measure_line.a,
            ],
            playhead: [
                lumino_gfx::colors::AR_PLAYHEAD_COLOR.0,
                lumino_gfx::colors::AR_PLAYHEAD_COLOR.1,
                lumino_gfx::colors::AR_PLAYHEAD_COLOR.2,
                lumino_gfx::colors::AR_PLAYHEAD_COLOR.3,
            ],
            sel_rect: [
                lumino_ui_core::constants::editor::SELECTION_BOX_FILL_COLOR.r,
                lumino_ui_core::constants::editor::SELECTION_BOX_FILL_COLOR.g,
                lumino_ui_core::constants::editor::SELECTION_BOX_FILL_COLOR.b,
            ],
        };

        // 走带视图轨道颜色：统一使用当前调色板（与 ArrangementUniform 一致），
        // 避免与 gfx 硬编码 ARRANGEMENT_PALETTE 双轨制造成颜色不同步。
        let track_colors: [[f32; 3]; 12] = std::array::from_fn(|i| {
            let c = lumino_extras::palette::current_track_color_f32(i);
            [c[0], c[1], c[2]]
        });

        let scene_params = ArrangementSceneParams {
            viewport: &viewport,
            track_order: &track_order,
            track_colors: &track_colors,
            track_visible: &track_visible,
            midi_doc: self.root.editor.editor_state.data.document.as_ref(),
            playback_position: self.root.editor.playback_position,
            colors: &colors,
            ghost_notes: &self.root.arrangement_view.ghost_notes,
            sel_rect: self
                .root
                .editor
                .editor_state
                .data
                .arrange_selection
                .rects
                .first()
                .map(|&(ts, te, _kl, _kh, tl, th)| {
                    (ts as f64, te as f64, tl as usize, th as usize)
                }),
            drag_sel_rect: self.root.arrangement_view.drag_sel_rect,
            time_signatures: &self.root.editor.editor_state.data.time_signatures,
        };

        lumino_gfx::collect_arrangement_instances(&scene_params)
    }

    /// 收集渲染所需的数据
    pub(crate) fn collect_render_data(&mut self) -> RenderData {
        let viewport_size = self.render_ctx.viewport.logical_size();

        // 走带模式下，同步视口到 canvas_size/canvas_offset 到 arrangement_view.viewport
        // 这些值用于 handlers.rs 的滚动范围钳制和 view.rs 的滚动条滑块计算
        // 而 collect_viewport_info() 每帧计算正确值但不会自动写回
        if self.root.is_arrangement_mode() {
            const TRACK_LIST_WIDTH: f32 = 160.0;
            const STATUSBAR_HEIGHT: f32 = 20.0;
            const TITLEBAR_HEIGHT: f32 = 30.0;
            const H_SCROLLBAR_HEIGHT: f32 = 20.0;
            const V_SCROLLBAR_WIDTH: f32 = 12.0;
            let sidebar_width = self.root.sidebar.width() as f32;
            let th = self.root.toolbar.height();
            let tbo = if cfg!(target_os = "macos") {
                0.0
            } else {
                TITLEBAR_HEIGHT
            };
            self.root.arrangement_view.viewport.canvas_size = iced_core::Point::new(
                (viewport_size.width - sidebar_width - TRACK_LIST_WIDTH - V_SCROLLBAR_WIDTH)
                    .max(1.0),
                (viewport_size.height - th - STATUSBAR_HEIGHT - H_SCROLLBAR_HEIGHT - tbo).max(1.0),
            );
            self.root.arrangement_view.viewport.canvas_offset =
                iced_core::Point::new(sidebar_width + TRACK_LIST_WIDTH, th + tbo);
        }

        let (scroll, zoom) = if self.root.is_arrangement_mode() {
            let av = &self.root.arrangement_view.viewport;
            ((av.scroll_x, av.scroll_y), (av.zoom_x, av.zoom_y))
        } else {
            let editor = &self.root.editor;
            (editor.scroll(), editor.zoom())
        };

        // 标尺实例由 GPU 线程的 RulerRenderer::prepare 内部基于缓存生成，
        // UI 线程不再每帧重复生成——之前每帧调用 generate_ruler_instances 是冗余的：
        // 1. 生成内容完全没被 GPU 使用（仅取 is_empty/len 判断）
        // 2. 与 GPU 端 cached_instances 的参数（keyboard_width 等）不一致时长度可能不同，
        //    作为 instance_count 传给 draw 会读到 buffer 末尾的垃圾数据
        // 3. 滚动/缩放时每帧 O(N) CPU 工作 + Vec 分配，纯属浪费
        // 字段保留为空 Vec 以维持 RenderData 结构体兼容性。
        let ruler_instances = vec![];

        self.update_note_data_for_wgpu_thread();

        // 收集走带视图音符实例
        let arrangement_note_instances = if self.root.is_arrangement_mode() {
            puffin::profile_scope!("collect_arrangement_instances");
            self.collect_arrangement_instances()
        } else {
            vec![]
        };

        // 构建 CC 柱状条实例（背景/网格/中心线）
        // 仅在自动化面板可见时构建，否则跳过 308ms 的无效计算
        let cc_bar_instances =
            if self.root.is_arrangement_mode() || !self.root.sidebar.automation_panel_visible {
                vec![]
            } else {
                puffin::profile_scope!("build_cc_bar_instances");
                self.build_cc_bar_instances()
            };

        // 洋葱皮流式上传：由 stream_onion_skin_instances 在检测到变化时
        // 分块构建 + send 到 WGPU 线程的 streaming channel，不再通过 RenderData 传输。
        self.stream_onion_skin_instances();

        RenderData {
            scroll,
            zoom,
            viewport_size,
            ruler_instances,
            arrangement_note_instances,
            cc_bar_instances,
        }
    }
}
