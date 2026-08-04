//! 渲染数据收集与参数构建 — 收集各类 GPU 实例数据并构建渲染参数
//!
//! 包含 Host 的以下方法：
//! - `collect_render_data`: 收集渲染所需数据
//! - `collect_arrangement_instances`: 收集走带视图实例
//! - `build_cc_bar_instances`: 构建 CC 柱状条实例
//! - `update_note_data_for_wgpu_thread`: 更新音符数据（双缓冲）
//! - `build_render_params`: 构建渲染参数

use crate::RenderParams;
use crate::host::Host;
use crate::host::render::data::{GridColors, RenderData};
use crate::host::render::note_worker;
use lumino_gfx::{
    ARRANGEMENT_PALETTE, ArrangementNoteInstance, ArrangementSceneParams, ArrangementUniform,
    ArrangementViewColors, CcBarColors, CcBarData, CcBarViewParams,
};

impl Host {
    /// 收集走带视图全部实例（背景 + lane + 网格线 + 音符 + 演奏指示线）
    /// 屏幕坐标，每帧重建，二分查找加速 MidiDocument 音符读取
    pub(super) fn collect_arrangement_instances(&self) -> Vec<ArrangementNoteInstance> {
        puffin::profile_scope!("collect_arrangement_instances");

        let track_order: Vec<usize> = self.root.sidebar.tracks.iter().map(|t| t.id).collect();
        let track_notes = &self.root.editor.editor_state.data.track_notes;
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
                theme.extended_palette().secondary.strong.color.r,
                theme.extended_palette().secondary.strong.color.g,
                theme.extended_palette().secondary.strong.color.b,
            ],
        };

        let scene_params = ArrangementSceneParams {
            viewport: &viewport,
            track_order: &track_order,
            track_colors: &ARRANGEMENT_PALETTE,
            track_visible: &track_visible,
            midi_doc: self.root.midi.document.as_deref(),
            track_notes,
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
        };

        lumino_gfx::collect_arrangement_instances(&scene_params)
    }

    /// 收集渲染所需的数据
    pub(super) fn collect_render_data(&mut self) -> RenderData {
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

        // WGPU 渲染模式下不使用 Iced Canvas 键盘
        let keyboard_instances = vec![];

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

        RenderData {
            scroll,
            zoom,
            viewport_size,
            keyboard_instances,
            ruler_instances,
            arrangement_note_instances,
            cc_bar_instances,
        }
    }

    /// 构建 CC 柱状条实例（背景/网格/中心线）
    fn build_cc_bar_instances(&self) -> Vec<lumino_gfx::CcBarInstance> {
        use crate::editor::grid::theme::ThemeExt;
        use crate::editor::velocity::EditMode;

        let editor = &self.root.editor;
        let panel = &editor.velocity_panel;

        // 根据编辑模式获取数据点和模式参数
        let (is_bend, is_velocity, cc_number) = match panel.edit_mode {
            EditMode::Bend => (true, false, 0u8),
            EditMode::Cc(n) => (false, false, n),
            EditMode::Velocity => (false, true, 0u8),
            EditMode::Tempo => (false, false, 0u8),
        };

        // Velocity 模式从 notes 获取力度点
        let velocity_points = if is_velocity {
            crate::editor::velocity::VelocityPanel::build_velocity_points(
                &editor.editor_state.data.notes,
            )
        } else {
            Vec::new()
        };

        // CC/Bend 模式从 automation_lanes 获取控制点
        let (cc_points, bend_points) = if is_bend {
            let bend_pts = crate::editor::velocity::VelocityPanel::build_bend_points(editor);
            (Vec::new(), bend_pts)
        } else if !is_velocity {
            let cc_pts = crate::editor::velocity::VelocityPanel::build_cc_points(editor, cc_number);
            (cc_pts, Vec::new())
        } else {
            (Vec::new(), Vec::new())
        };

        let track_idx = editor.editor_state.data.current_track as u16;
        let automation_lane = if is_bend {
            editor
                .editor_state
                .data
                .find_automation_lane(track_idx, &lumino_note_core::AutomationTarget::PitchBend)
                .and_then(|idx| {
                    editor
                        .editor_state
                        .data
                        .automation_lanes
                        .get(idx)
                        .map(|a| &**a)
                })
        } else if !is_velocity {
            editor
                .editor_state
                .data
                .find_automation_lane(
                    track_idx,
                    &lumino_note_core::AutomationTarget::CC {
                        controller: cc_number,
                    },
                )
                .and_then(|idx| {
                    editor
                        .editor_state
                        .data
                        .automation_lanes
                        .get(idx)
                        .map(|a| &**a)
                })
        } else {
            None
        };

        let view = &editor.editor_state.view;
        let canvas = &editor.editor_state.canvas;
        let theme = self.root.theme();
        let panel_height = self.root.visual.velocity_panel_height;

        // 颜色
        let note_color = theme.extended_palette().primary.weak.color;
        let bar_color = [note_color.r, note_color.g, note_color.b, 0.30];

        let bg = theme.extended_palette().background.base.color;
        let bg_color = [bg.r, bg.g, bg.b, 1.0];

        let handle = theme.extended_palette().background.strong.color;
        let handle_color = [handle.r, handle.g, handle.b, 0.25];

        let text_c = theme.text_color();
        let grab_alpha = if theme.is_light() { 0.40 } else { 0.35 };
        let grab_color = [text_c.r, text_c.g, text_c.b, grab_alpha];

        let cc_view_params = CcBarViewParams {
            panel_height,
            keyboard_width: view.keyboard_width,
            scroll_x: view.scroll_x,
            zoom_x: view.zoom_x,
            canvas_offset_x: canvas.offset_x,
            canvas_offset_y: canvas.offset_y,
            canvas_size_x: canvas.size_x,
            canvas_size_y: canvas.size_y,
            value_zoom: panel.value_zoom,
            value_scroll: panel.value_scroll,
            line_thickness: panel.automation_line_thickness,
        };
        let cc_colors = CcBarColors {
            bar_color,
            bg_color,
            handle_color,
            grab_color,
        };
        let cc_data = CcBarData {
            velocity_points: &velocity_points,
            cc_points: &cc_points,
            bend_points: &bend_points,
            automation_lane,
            velocity_curve_style: self.root.settings.velocity_curve_style,
        };

        lumino_gfx::build_cc_bar_instances(&panel.edit_mode, &cc_view_params, &cc_data, &cc_colors)
    }

    /// 更新 WGPU 渲染线程的音符数据（双缓冲 + 异步计算）
    ///
    /// 优化策略：
    /// 1. CPU 端可见性裁剪：仅构建视口内（含 overscan）的音符实例
    /// 2. Overscan 缓存：若当前视口仍在上一次渲染的扩展视口内且数据未变，跳过重建
    pub(super) fn update_note_data_for_wgpu_thread(&mut self) {
        puffin::profile_scope!("update_note_data");
        const OVERSCAN_FACTOR: f32 = 0.5;

        // 走带模式使用 arrangement_renderer，不需要音符实例
        if self.root.is_arrangement_mode() {
            return;
        }

        let note_index_dirty = self.root.editor.spatial.note_index_dirty.get();
        let current_edit_state = self.root.editor.editor_state.interaction.edit_state.clone();
        let is_drawing = matches!(current_edit_state, crate::editor::EditState::Drawing { .. });
        // DraggingSelection 期间 has_active_ghost_delta 返回 false（ghost 方案设计如此），
        // 音符不会被 ghost，渲染结果与无拖拽时完全相同。因此不将其标记为 ghost_dragging，
        // 避免每帧触发 collect_visible_note_data 和 note instances 重建。
        let is_ghost_dragging = matches!(
            current_edit_state,
            crate::editor::EditState::Dragging { .. }
        ) || self.root.editor.has_pending_drag();

        // 计算视口哈希
        let editor_view = &self.root.editor.editor_state.view;
        let canvas = &self.root.editor.editor_state.canvas;
        let current_viewport_hash = crate::host::RenderCache::compute_viewport_hash(
            editor_view.scroll_x,
            editor_view.scroll_y,
            editor_view.zoom_x,
            editor_view.zoom_y,
            canvas.size_x,
            canvas.size_y,
            editor_view.visible_key_count,
        );
        let viewport_changed =
            current_viewport_hash != self.render_ctx.render_cache.note_viewport_hash;

        // hover 预览（铅笔工具 + Idle）时光标移动也需要重建实例
        let cursor_changed =
            self.window_ctx.cursor_position != self.render_ctx.last_cursor_position;
        let is_hover_preview = matches!(current_edit_state, crate::editor::EditState::Idle)
            && self.root.editor.current_tool() == crate::message::Tool::Pencil
            && self.root.should_render_preview_note();
        let note_data_changed = note_index_dirty
            || self.render_ctx.render_cache.note_instances_is_empty()
            || is_drawing
            || is_ghost_dragging
            || (is_hover_preview && cursor_changed);

        // 计算当前精确视口范围
        let (tick_start, tick_end, key_min, key_max) = self.root.editor.compute_visible_range(0.0);

        // 若数据未变且当前视口在缓存的渲染视口内，跳过重建
        if !note_data_changed
            && !viewport_changed
            && self
                .render_ctx
                .render_cache
                .note_render_viewport
                .as_ref()
                .is_some_and(|vp| vp.contains(tick_start, tick_end, key_min, key_max))
        {
            return;
        }

        self.render_ctx.render_cache.note_viewport_hash = current_viewport_hash;

        // Phase 1: 主音符实例构建（仅构建可见音符）
        if note_data_changed || viewport_changed {
            puffin::profile_scope!("phase1_main_notes_sync");
            let edit_state_clone = self.root.editor.editor_state.interaction.edit_state.clone();
            let default_note_length = self.root.editor.editor_state.view.default_note_length;
            let snap_precision = self.root.editor.editor_state.view.snap_precision;

            // 构建预览音符上下文
            let preview_ctx = {
                let editor = &self.root.editor;
                let view = &editor.editor_state.view;
                let canvas = &editor.editor_state.canvas;
                let is_idle = matches!(current_edit_state, crate::editor::EditState::Idle);
                let is_pencil = editor.current_tool() == crate::message::Tool::Pencil;
                let should_render = self.root.should_render_preview_note();

                let (cursor_tick, cursor_key, cursor_in_canvas) =
                    if let Some((cx, cy)) = canvas.cursor_position {
                        let local_x = cx - canvas.offset_x;
                        let local_y = cy - canvas.offset_y;
                        let in_canvas = local_x >= view.keyboard_width
                            && local_y >= view.ruler_height
                            && local_x < canvas.size_x
                            && local_y < canvas.size_y;
                        let tick = view.snap_tick(view.x_to_tick(local_x)).max(0.0);
                        let key = view.y_to_key(local_y);
                        (tick, key, in_canvas)
                    } else {
                        (0.0, 0u16, false)
                    };

                let hover_preview = is_idle && is_pencil && should_render && cursor_in_canvas;

                note_worker::PreviewNoteContext {
                    hover_preview,
                    cursor_tick,
                    cursor_key,
                    last_note_length: view.last_note_length,
                }
            };

            let visible_count = self.root.editor.collect_visible_note_data(
                &mut self.render_ctx.render_cache.visible_notes_buffer,
                OVERSCAN_FACTOR,
            );
            let visible_notes = &self.render_ctx.render_cache.visible_notes_buffer;
            // wasabi 风格 border_width：键轴方向像素长度 / 可见键数
            // lumino 钢琴卷帘键轴垂直 → 用画布高度（减标尺）映射 wasabi 的 width_pixels
            let view = &self.root.editor.editor_state.view;
            let canvas = &self.root.editor.editor_state.canvas;
            let key_axis_pixels = (canvas.size_y - view.ruler_height).max(1.0);
            let border_width =
                lumino_gfx::calculate_border_width(key_axis_pixels, view.visible_key_count as f32);
            note_worker::build_main_note_instances(
                &self.render_ctx.render_cache.note_instances_buffer,
                visible_notes,
                &edit_state_clone,
                default_note_length,
                snap_precision,
                &preview_ctx,
                border_width,
            );
            tracing::debug!(
                "WGPU thread: built {} visible note instances from expanded query",
                visible_count
            );
        }

        // 更新光标位置缓存
        self.render_ctx.last_cursor_position = self.window_ctx.cursor_position;

        // 更新缓存的渲染视口为本次使用的扩展视口
        let (render_tick_start, render_tick_end, render_key_min, render_key_max) =
            self.root.editor.compute_visible_range(OVERSCAN_FACTOR);
        self.render_ctx.render_cache.note_render_viewport =
            Some(crate::host::cache::NoteRenderViewport {
                tick_start: render_tick_start,
                tick_end: render_tick_end,
                key_min: render_key_min,
                key_max: render_key_max,
            });

        // 滚动速度追踪保留，供未来 overscan 预测使用
        let _velocity = self
            .scroll_tracker
            .update(editor_view.scroll_x, editor_view.zoom_x);
        let _ = _velocity;
    }

    /// 构建渲染参数
    pub(super) fn build_render_params(&mut self, data: RenderData) -> RenderParams {
        use crate::editor::velocity::PANEL_PADDING_Y;
        let es = &self.root.editor.editor_state;
        let physical_size = self.render_ctx.viewport.physical_size();
        let theme = self.root.theme();
        let colors = GridColors::from_theme(&theme);

        let bg_color = [
            colors.bg[0] as f64,
            colors.bg[1] as f64,
            colors.bg[2] as f64,
            colors.bg[3] as f64,
        ];
        let ppq = es.view.ppq;
        let is_arrangement_mode = self.root.is_arrangement_mode();
        let max_key_index = if is_arrangement_mode {
            let av = &self.root.arrangement_view.viewport;
            let track_count = self.root.sidebar.tracks.len().max(1) as f32;
            track_count * av.track_height - av.track_height / 128.0
        } else {
            (es.view.visible_key_count.saturating_sub(1)) as f32
        };

        // 从 collect_viewport_info 获取 canvas_offset 和 canvas_size
        let viewport_info = self.collect_viewport_info();
        let (canvas_offset, canvas_size, keyboard_width, ruler_height) = if is_arrangement_mode {
            (
                (viewport_info.canvas_offset.x, viewport_info.canvas_offset.y),
                (viewport_info.canvas_size.x, viewport_info.canvas_size.y),
                0.0,
                0.0,
            )
        } else {
            (
                (es.canvas.offset_x, es.canvas.offset_y),
                (es.canvas.size_x, es.canvas.size_y),
                es.view.keyboard_width,
                es.view.ruler_height,
            )
        };

        // 构建 arrangement uniform
        let bg_color_arr = colors.bg;
        let bar_color = colors.bar_line;
        let arrangement_uniform = if is_arrangement_mode {
            let av = &self.root.arrangement_view.viewport;
            let track_count = self.root.sidebar.tracks.len().max(1) as f32;
            let mut track_colors = [[0.0_f32; 4]; 16];
            // 使用当前调色板的颜色（来自 PaletteManager），
            // 超出调色板颜色数的轨道循环取色
            for (i, slot) in track_colors.iter_mut().enumerate() {
                *slot = lumino_extras::palette::current_track_color_f32(i);
            }
            let playhead_x = if self.root.editor.playback_position > 0.0 {
                self.root.editor.playback_position * av.zoom_x - data.scroll.0
            } else {
                -1.0
            };
            ArrangementUniform {
                scroll: [data.scroll.0, data.scroll.1],
                zoom: data.zoom.0,
                track_height: av.track_height,
                viewport_size: [data.viewport_size.width, data.viewport_size.height],
                canvas_offset: [canvas_offset.0, canvas_offset.1],
                playhead_x,
                bg_color: [
                    bg_color_arr[0],
                    bg_color_arr[1],
                    bg_color_arr[2],
                    bg_color_arr[3],
                ],
                bar_color: [bar_color[0], bar_color[1], bar_color[2], bar_color[3]],
                playhead_color: [
                    lumino_gfx::colors::AR_PLAYHEAD_COLOR.0,
                    lumino_gfx::colors::AR_PLAYHEAD_COLOR.1,
                    lumino_gfx::colors::AR_PLAYHEAD_COLOR.2,
                    lumino_gfx::colors::AR_PLAYHEAD_COLOR.3,
                ],
                track_colors,
                track_count,
                ..Default::default()
            }
        } else {
            ArrangementUniform::default()
        };

        // 计算力度面板矩形（用于 wgpu scissor 裁剪）
        // 仅在自动化面板可见时设置，否则跳过 CC bar 渲染器 prepare/draw
        let velocity_panel_rect =
            if is_arrangement_mode || !self.root.sidebar.automation_panel_visible {
                None
            } else {
                let es = &self.root.editor.editor_state;
                // velocity 面板在 grid Canvas 下方，间隔 0px
                const H_SCROLLBAR_HEIGHT: f32 = 20.0;
                Some((
                    es.canvas.offset_x,
                    es.canvas.offset_y + es.canvas.size_y + H_SCROLLBAR_HEIGHT,
                    es.canvas.size_x,
                    self.root.visual.velocity_panel_height + PANEL_PADDING_Y + 10.0,
                ))
            };

        RenderParams::builder()
            .viewport_size((physical_size.width, physical_size.height))
            .logical_size((data.viewport_size.width, data.viewport_size.height))
            .scale_factor(self.render_ctx.viewport.scale_factor())
            .scroll(data.scroll)
            .zoom(data.zoom)
            .keyboard_width(keyboard_width)
            .ruler_height(ruler_height)
            .canvas_offset((canvas_offset.0, canvas_offset.1))
            .canvas_size((canvas_size.0, canvas_size.1))
            .background_color(bg_color)
            .color_bg(colors.bg)
            .color_bg_black_key(colors.black_key)
            .color_bar(colors.bar_line)
            .color_beat(colors.beat_line)
            .color_half_beat(colors.half_beat_line)
            .color_grid(colors.grid_line)
            .color_key_line(colors.key_line)
            .ppq(ppq as f32)
            .max_key_index(max_key_index)
            .is_arrangement_mode(is_arrangement_mode)
            .ruler_instances(data.ruler_instances)
            .time_signatures(es.data.time_signatures.clone())
            .keyboard_instances(data.keyboard_instances)
            .arrangement_note_instances(data.arrangement_note_instances)
            .arrangement_uniform(arrangement_uniform)
            .cc_bar_instances(data.cc_bar_instances)
            .velocity_panel_rect(velocity_panel_rect)
            .build()
    }
}
