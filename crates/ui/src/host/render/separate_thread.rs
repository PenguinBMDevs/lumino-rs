use crate::RenderParams;
use crate::host::Host;
use lumino_gfx::{
    ARRANGEMENT_PALETTE, ArrangementNoteInstance, ArrangementSceneParams, ArrangementUniform,
    ArrangementViewColors, CcBarColors, CcBarData, CcBarViewParams, OnionNote,
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
        };

        lumino_gfx::collect_arrangement_instances(&scene_params)
    }

    /// 分离渲染线程模式的主渲染入口
    pub(super) fn render_with_separate_thread(
        &mut self,
        frame: &iced_wgpu::wgpu::SurfaceTexture,
        gfx: &lumino_gfx::Context,
    ) {
        self.redraw_separate_thread();

        // 将离屏渲染结果拷贝到 Surface
        if let Some(ref wgpu_thread) = self.render_ctx.wgpu_render_thread {
            wgpu_thread.copy_offscreen_to_surface(frame, &gfx.device, &gfx.queue);
        }

        // iced UI 覆盖层渲染到同一 surface
        if !self.skip_ui_rendering {
            let view = frame
                .texture
                .create_view(&iced_wgpu::wgpu::TextureViewDescriptor::default());
            self.render_iced_ui(frame, &view);
        }
    }

    /// 分离渲染线程模式的主渲染逻辑
    ///
    /// UI 线程只负责：
    /// 1. 更新状态
    /// 2. 生成渲染参数
    /// 3. 写入音符数据到双缓冲
    /// 4. 发送渲染参数到 WGPU 线程
    pub(super) fn redraw_separate_thread(&mut self) {
        puffin::profile_function!();
        puffin::profile_scope!("redraw_separate_thread");

        if !self.validate_render_thread_ready() {
            return;
        }

        let render_data = self.collect_render_data();
        let params = self.build_render_params(render_data);

        // 发送渲染参数到 WGPU 线程（非阻塞）
        if let Some(ref wgpu_thread) = self.render_ctx.wgpu_render_thread {
            wgpu_thread.send_params(params);
        }
    }

    /// 验证渲染线程是否就绪
    pub(super) fn validate_render_thread_ready(&self) -> bool {
        if self.render_ctx.wgpu_render_thread.is_none() {
            tracing::error!("redraw_separate_thread called but wgpu_render_thread is None");
            return false;
        }

        true
    }

    /// 收集渲染所需的数据
    pub(super) fn collect_render_data(&mut self) -> super::data::RenderData {
        let viewport_size = self.render_ctx.viewport.logical_size();

        // 走带模式下，同步视口到 canvas_size/canvas_offset 到 arrangement_view.viewport
        // 这些值用于 handlers.rs 的滚动范围钳制和 view.rs 的滚动条滑块计算
        // 而 collect_viewport_info() 每帧计算正确值但不会自动写回
        if self.root.is_arrangement_mode() {
            const TRACK_LIST_WIDTH: f32 = 160.0;
            const STATUSBAR_HEIGHT: f32 = 20.0;
            const TITLEBAR_HEIGHT: f32 = 30.0;
            let th = self.root.toolbar.height();
            let tbo = if cfg!(target_os = "macos") {
                0.0
            } else {
                TITLEBAR_HEIGHT
            };
            const H_SCROLLBAR_HEIGHT: f32 = 20.0;
            self.root.arrangement_view.viewport.canvas_size = iced_core::Point::new(
                (viewport_size.width - TRACK_LIST_WIDTH).max(1.0),
                (viewport_size.height - th - STATUSBAR_HEIGHT - H_SCROLLBAR_HEIGHT - tbo).max(1.0),
            );
            self.root.arrangement_view.viewport.canvas_offset =
                iced_core::Point::new(TRACK_LIST_WIDTH, th + tbo);
        }

        let (scroll, zoom) = if self.root.is_arrangement_mode() {
            let av = &self.root.arrangement_view.viewport;
            ((av.scroll_x, av.scroll_y), (av.zoom_x, av.zoom_y))
        } else {
            let editor = &self.root.editor;
            (editor.scroll(), editor.zoom())
        };

        let grid_instances = if self.root.is_arrangement_mode() {
            vec![] // 走带模式不使用网格
        } else {
            puffin::profile_scope!("generate_grid_instances");
            let grid_params = lumino_gfx::GridViewParams {
                viewport_width: viewport_size.width,
                viewport_height: viewport_size.height,
                keyboard_width: super::DEFAULT_KEYBOARD_WIDTH,
                ruler_height: super::DEFAULT_RULER_HEIGHT,
                scroll_x: scroll.0,
                scroll_y: scroll.1,
                zoom_x: zoom.0,
                zoom_y: zoom.1,
            };
            self.generate_grid_instances(&grid_params)
        };

        // WGPU 渲染模式下不使用 Iced Canvas 键盘
        let keyboard_instances = vec![];

        let ruler_instances = if self.root.is_arrangement_mode() {
            vec![] // 走带模式不使用标尺
        } else {
            puffin::profile_scope!("generate_ruler_instances");
            self.generate_ruler_instances(
                viewport_size.width,
                super::DEFAULT_KEYBOARD_WIDTH,
                super::DEFAULT_RULER_HEIGHT,
                scroll.0,
                zoom.0,
            )
        };

        self.update_note_data_for_wgpu_thread();

        // 收集走带视图音符实例
        let arrangement_note_instances = if self.root.is_arrangement_mode() {
            puffin::profile_scope!("collect_arrangement_instances");
            self.collect_arrangement_instances()
        } else {
            vec![]
        };

        // 构建 CC 柱状条实例（背景/网格/中心线）
        let cc_bar_instances = if self.root.is_arrangement_mode() {
            vec![]
        } else {
            puffin::profile_scope!("build_cc_bar_instances");
            self.build_cc_bar_instances()
        };

        super::data::RenderData {
            scroll,
            zoom,
            viewport_size,
            grid_instances,
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

        // CC/Bend 模式从 cc_data 获取控制点
        let (cc_points, bend_points) = if is_bend {
            let bend_pts = crate::editor::velocity::VelocityPanel::build_bend_points(editor);
            (Vec::new(), bend_pts)
        } else if !is_velocity {
            let cc_pts = crate::editor::velocity::VelocityPanel::build_cc_points(editor, cc_number);
            (cc_pts, Vec::new())
        } else {
            (Vec::new(), Vec::new())
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
            notes: &editor.editor_state.data.notes,
        };

        lumino_gfx::build_cc_bar_instances(&panel.edit_mode, &cc_view_params, &cc_data, &cc_colors)
    }

    /// 更新 WGPU 渲染线程的音符数据（双缓冲 + 异步计算）
    ///
    /// Phase 1: 主音符实例构建（同步，O(N) ~1ms）
    ///   直接写入双缓冲的 write buffer，然后 swap
    /// Phase 2: 洋葱皮实例构建（异步，发送给 NoteWorker 后台计算后 swap）
    ///   耗时 50-200ms 的复杂计算在后台进行
    pub(super) fn update_note_data_for_wgpu_thread(&mut self) {
        puffin::profile_scope!("update_note_data");

        // 走带模式使用 arrangement_renderer，不需要音符实例
        if self.root.is_arrangement_mode() {
            return;
        }

        let note_index_dirty = self.root.editor.spatial.note_index_dirty.get();
        let is_drawing = matches!(
            self.root.editor.editor_state.interaction.edit_state,
            crate::editor::EditState::Drawing { .. }
        );

        // 计算视口哈希
        let v = &self.root.editor.editor_state.view;
        let canvas = &self.root.editor.editor_state.canvas;
        let current_viewport_hash = crate::host::RenderCache::compute_viewport_hash(
            v.scroll_x,
            v.scroll_y,
            v.zoom_x,
            v.zoom_y,
            canvas.size_x,
            canvas.size_y,
            v.visible_key_count,
        );
        let viewport_changed =
            current_viewport_hash != self.render_ctx.render_cache.note_viewport_hash;

        // 洋葱皮视口哈希（32px 偏移容差，减少不必要的重算）
        let current_onion_hash = crate::host::RenderCache::compute_onion_viewport_hash(
            v.scroll_x,
            v.scroll_y,
            v.zoom_x,
            v.zoom_y,
            canvas.size_x,
            canvas.size_y,
            v.visible_key_count,
        );
        let onion_viewport_changed =
            current_onion_hash != self.render_ctx.render_cache.onion_viewport_hash;

        let note_data_changed = note_index_dirty
            || self.render_ctx.render_cache.note_instances_is_empty()
            || is_drawing;

        if !note_data_changed && !viewport_changed {
            return;
        }

        self.render_ctx.render_cache.note_viewport_hash = current_viewport_hash;

        // Phase 1: 主音符实例构建（同步）
        // 直接写入双缓冲的 write buffer，然后 swap
        if note_data_changed {
            puffin::profile_scope!("phase1_main_notes_sync");
            let notes_clone = self.root.editor.editor_state.data.notes.clone(); // O(1)
            let edit_state_clone = self.root.editor.editor_state.interaction.edit_state.clone();
            let default_note_length = self.root.editor.editor_state.view.default_note_length;
            let snap_precision = self.root.editor.editor_state.view.snap_precision;
            super::note_worker::build_main_note_instances(
                &self.render_ctx.render_cache.note_instances_buffer,
                &notes_clone,
                &edit_state_clone,
                default_note_length,
                snap_precision,
            );
        }

        // 提取 scroll 值（避免 update_onion_note_list 的 &mut self 与 v 的借用冲突）
        let scroll_x = v.scroll_x;
        let zoom_x = v.zoom_x;

        // 增量维护洋葱皮音符列表（参考 Wasabi 瀑布流简化方案）
        let _list_changed = self.update_onion_note_list();

        // 更新滚动速度追踪（用于 overscan 计算，在 build_render_params 中读取）
        let _velocity = self.scroll_tracker.update(scroll_x, zoom_x);

        // 方案 C：渲染线程负责采集，主线程只更新视口哈希
        if note_data_changed || onion_viewport_changed {
            self.render_ctx.render_cache.onion_viewport_hash = current_onion_hash;
        }

        if note_index_dirty {
            self.root.editor.spatial.note_index_dirty.set(false);
        }
    }

    /// 构建渲染参数
    pub(super) fn build_render_params(&mut self, data: super::data::RenderData) -> RenderParams {
        use crate::editor::velocity::PANEL_PADDING_Y;
        let es = &self.root.editor.editor_state;
        let physical_size = self.render_ctx.viewport.physical_size();
        let theme = self.root.theme();
        let colors = super::data::GridColors::from_theme(&theme);

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
            for (i, &c) in ARRANGEMENT_PALETTE.iter().enumerate().take(16) {
                track_colors[i] = [c[0], c[1], c[2], 1.0];
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
        let velocity_panel_rect = if is_arrangement_mode {
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

        // ── 洋葱皮 per-track 打包颜色 ──
        // 仅在颜色配置/音轨数量变化时重建，避免每帧 O(track_count) 分配
        let onion_track_colors = {
            let cache = &mut self.render_ctx.render_cache;
            let onion_skin_colors = &self.root.editor.onion_skin.config.colors;
            let track_count = self.root.sidebar.tracks.len();
            let colors_dirty = cache
                .onion_track_colors
                .as_ref()
                .is_none_or(|c| c.len() != track_count)
                || cache.onion_colors_version != onion_skin_colors.version();

            if colors_dirty {
                let mut packed_colors: Vec<u32> = Vec::with_capacity(track_count);
                for i in 0..track_count {
                    let c = onion_skin_colors.get_raw(i);
                    packed_colors.push(OnionNote::pack_rgba(c.r, c.g, c.b, c.a));
                }
                cache.onion_track_colors = Some(packed_colors);
                cache.onion_colors_version = onion_skin_colors.version();
            }

            cache.onion_track_colors.clone()
        };

        // ── 洋葱皮渲染线程采集参数 ──
        let onion_note_list = self
            .render_ctx
            .render_cache
            .onion_note_list
            .as_ref()
            .map(std::sync::Arc::clone);
        let onion_list_version = onion_note_list.as_ref().map_or(0, |l| l.version());
        let onion_overscan_ticks = self.scroll_tracker.overscan_ticks(60.0);
        let onion_current_track = es.data.current_track as u16;
        let onion_enabled = self.root.editor.is_onion_skin_enabled();

        RenderParams::from_data(
            (physical_size.width, physical_size.height),
            (data.viewport_size.width, data.viewport_size.height),
            self.render_ctx.viewport.scale_factor(),
            data.scroll,
            data.zoom,
            keyboard_width,
            ruler_height,
            (canvas_offset.0, canvas_offset.1),
            (canvas_size.0, canvas_size.1),
            bg_color,
            colors.bg,
            colors.black_key,
            colors.bar_line,
            colors.beat_line,
            colors.half_beat_line,
            colors.grid_line,
            colors.key_line,
            ppq as f32,
            max_key_index,
            is_arrangement_mode,
            data.grid_instances,
            data.ruler_instances,
            data.keyboard_instances,
            data.arrangement_note_instances,
            arrangement_uniform,
            data.cc_bar_instances,
            velocity_panel_rect,
            onion_track_colors,
            onion_note_list,
            onion_list_version,
            onion_overscan_ticks,
            onion_current_track,
            onion_enabled,
        )
    }

    // build_velocity_graph_instances 已迁移到 build_cc_bar_instances
}
