use crate::RenderParams;
use crate::host::Host;
use iced_wgpu::wgpu;
use lumino_gfx::{ArrangementNoteInstance, ArrangementUniform};

mod arrangement_instances;

/// 走带视图音轨调色板（12 色，与 view.rs 保持同步）
const ARRANGEMENT_PALETTE: [[f32; 3]; 12] = [
    [0.90, 0.30, 0.30], // 红
    [0.30, 0.70, 0.30], // 绿
    [0.30, 0.50, 0.90], // 蓝
    [0.90, 0.70, 0.20], // 橙
    [0.70, 0.30, 0.80], // 紫
    [0.20, 0.80, 0.80], // 青
    [0.90, 0.50, 0.50], // 粉红
    [0.50, 0.90, 0.30], // lime
    [0.30, 0.30, 0.70], // 深蓝
    [0.90, 0.80, 0.30], // 黄
    [0.60, 0.40, 0.20], // 棕
    [0.50, 0.50, 0.50], // 灰
];

impl Host {
    /// 收集走带视图全部实例（背景 + lane + 网格线 + 音符 + 演奏指示线）
    /// 屏幕坐标，每帧重建，二分查找加速 MidiDocument 音符读取
    pub(super) fn collect_arrangement_instances(&self) -> Vec<ArrangementNoteInstance> {
        puffin::profile_scope!("collect_arrangement_instances");

        let track_order: Vec<usize> = self.root.sidebar.tracks.iter().map(|t| t.id).collect();
        let track_notes = &self.root.editor.editor_state.data.track_notes;
        let viewport_info = self.collect_viewport_info();
        let av = &self.root.arrangement_view.viewport;

        let viewport = crate::editor::arrangement::ArrangementViewport {
            scroll_x: av.scroll_x,
            scroll_y: av.scroll_y,
            zoom_x: av.zoom_x,
            zoom_y: av.zoom_y,
            track_height: av.track_height,
            canvas_offset: viewport_info.canvas_offset,
            canvas_size: viewport_info.canvas_size,
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
        // 演奏指示线：DAW 标准红色，与钢琴卷帘统一
        let arr_playhead = iced_core::Color::from_rgba(
            lumino_gfx::colors::AR_PLAYHEAD_COLOR.0,
            lumino_gfx::colors::AR_PLAYHEAD_COLOR.1,
            lumino_gfx::colors::AR_PLAYHEAD_COLOR.2,
            lumino_gfx::colors::AR_PLAYHEAD_COLOR.3,
        );

        let mut instances = Vec::new();
        arrangement_instances::build_arrangement_all(
            &mut instances,
            &viewport,
            &track_order,
            &ARRANGEMENT_PALETTE,
            &track_visible,
            self.root.midi.document.as_ref().map(|v| &**v),
            track_notes,
            self.root.editor.playback_position as f32,
            [arr_bg.r, arr_bg.g, arr_bg.b],
            [arr_lane_even.r, arr_lane_even.g, arr_lane_even.b],
            [arr_lane_odd.r, arr_lane_odd.g, arr_lane_odd.b],
            [
                arr_measure_line.r,
                arr_measure_line.g,
                arr_measure_line.b,
                arr_measure_line.a,
            ],
            [
                arr_playhead.r,
                arr_playhead.g,
                arr_playhead.b,
                arr_playhead.a,
            ],
        );
        instances
    }

    /// 分离渲染线程模式
    pub(super) fn render_with_separate_thread(
        &mut self,
        frame: &wgpu::SurfaceTexture,
        gfx: &lumino_gfx::Context,
    ) {
        self.redraw_separate_thread();
        self.copy_offscreen_texture_to_surface(frame, gfx);

        // iced UI 仍然需要在主线程渲染到当前 surface
        if !self.skip_ui_rendering {
            let view = frame
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            self.render_iced_ui(frame, &view);
        }
    }

    /// 将离屏渲染结果复制到当前 Surface
    pub(super) fn copy_offscreen_texture_to_surface(
        &mut self,
        frame: &wgpu::SurfaceTexture,
        gfx: &lumino_gfx::Context,
    ) {
        let Some(ref wgpu_thread) = self.render_ctx.wgpu_render_thread else {
            return;
        };

        let texture_ref = wgpu_thread
            .latest_texture
            .try_lock()
            .ok()
            .and_then(|g| g.clone());

        let Some(texture) = texture_ref else {
            return;
        };

        if texture.width() != frame.texture.width() || texture.height() != frame.texture.height() {
            return;
        }

        puffin::profile_scope!("copy_offscreen_texture");
        let mut encoder = gfx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("copy_offscreen_texture_encoder"),
            });

        encoder.copy_texture_to_texture(
            texture.as_image_copy(),
            frame.texture.as_image_copy(),
            wgpu::Extent3d {
                width: texture.width(),
                height: texture.height(),
                depth_or_array_layers: 1,
            },
        );

        gfx.queue.submit(std::iter::once(encoder.finish()));
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

        if self.render_ctx.note_events_tx.is_none() {
            tracing::error!("redraw_separate_thread called but note_events_tx is None");
            return false;
        }

        true
    }

    /// 收集渲染所需的数据
    pub(super) fn collect_render_data(&mut self) -> super::data::RenderData {
        let viewport_size = self.render_ctx.viewport.logical_size();

        // 走带模式下，同步视口的 canvas_size/canvas_offset 到 arrangement_view.viewport
        // 这些值用于 handlers.rs 的滚动范围钳制和 view.rs 的滚动条滑块计算，
        // 而 collect_viewport_info() 每帧计算正确值但不会自动写回。
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
            vec![] // 音轨总览模式下跳过网格
        } else {
            puffin::profile_scope!("generate_grid_instances");
            self.generate_grid_instances(
                viewport_size.width,
                viewport_size.height,
                super::DEFAULT_KEYBOARD_WIDTH,
                super::DEFAULT_RULER_HEIGHT,
                scroll.0,
                scroll.1,
                zoom.0,
                zoom.1,
            )
        };

        // WGPU 键盘渲染已移除，使用 Iced Canvas 键盘替代
        let keyboard_instances = vec![];

        let ruler_instances = if self.root.is_arrangement_mode() {
            vec![] // 音轨总览模式下跳过标尺
        } else {
            puffin::profile_scope!("generate_ruler_instances");
            self.generate_ruler_instances(
                viewport_size.width,
                super::DEFAULT_KEYBOARD_WIDTH,
                super::DEFAULT_RULER_HEIGHT,
                scroll.0,
                zoom.0,
                super::TICKS_PER_MEASURE,
                super::TICKS_PER_BEAT,
            )
        };

        self.update_note_data_for_wgpu_thread();

        // 走带模式：直接构建全部实例
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

    /// 构建 CC 柱状条实例（参考 yinhe 计算方式）
    fn build_cc_bar_instances(&self) -> Vec<lumino_gfx::CcBarInstance> {
        use crate::editor::grid::theme::ThemeExt;
        use crate::editor::velocity::{EditMode, PANEL_PADDING_Y, RESIZE_HANDLE_HEIGHT};

        let editor = &self.root.editor;
        let panel = &editor.velocity_panel;
        let is_tempo = matches!(panel.edit_mode, EditMode::Tempo);

        // 根据模式获取数据点和模式参数
        let (is_bend, is_velocity, cc_number) = match panel.edit_mode {
            EditMode::Bend => (true, false, 0u8),
            EditMode::Cc(n) => (false, false, n),
            EditMode::Velocity => (false, true, 0u8),
            EditMode::Tempo => (false, false, 0u8),
        };

        // Velocity 模式：从 notes 获取力度点
        let velocity_points = if is_velocity {
            crate::editor::velocity::VelocityPanel::build_velocity_points(
                &editor.editor_state.data.notes,
            )
        } else {
            Vec::new()
        };

        // CC/Bend 模式：从 cc_data 获取数据点
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
        let theme = self.root.theme();
        let note_color = theme.extended_palette().primary.weak.color;
        let bar_color_arr = [note_color.r, note_color.g, note_color.b, 0.30];

        let canvas = &editor.editor_state.canvas;
        let panel_height = self.root.velocity_panel_height;
        let panel_x = canvas.offset.x;
        let panel_y = canvas.offset.y + canvas.size.y;
        // velocity 面板在 grid Canvas 下方，中间隔了水平滚动条（20px）
        const H_SCROLLBAR_HEIGHT: f32 = 20.0;
        let actual_panel_y = panel_y + H_SCROLLBAR_HEIGHT;

        let mut instances = Vec::new();

        // ── 所有模式共用的层 ──

        // 1. 背景
        let bg = theme.extended_palette().background.base.color;
        let bg_arr = [bg.r, bg.g, bg.b, 1.0];
        let bg_height = panel_height + PANEL_PADDING_Y + 10.0;
        instances.push(lumino_gfx::CcBarInstance::new(
            panel_x,
            actual_panel_y,
            canvas.size.x,
            bg_height,
            bg_arr,
        ));

        // 2. Resize 手柄（位于 toolbar 下方 = Canvas 顶部）
        const TOOLBAR_HEIGHT: f32 = 28.0;
        let handle_y = actual_panel_y + TOOLBAR_HEIGHT;
        let handle_color = theme.extended_palette().background.strong.color;
        instances.push(lumino_gfx::CcBarInstance::new(
            panel_x,
            handle_y,
            canvas.size.x,
            RESIZE_HANDLE_HEIGHT,
            [handle_color.r, handle_color.g, handle_color.b, 0.25],
        ));
        // Grab 指示条
        let text_c = theme.text_color();
        let grab_alpha = if theme.is_light() { 0.40 } else { 0.35 };
        let bar_w = 40.0;
        let bar_h = 3.0;
        let bar_x = panel_x + (canvas.size.x - bar_w) / 2.0;
        let bar_y = handle_y + (RESIZE_HANDLE_HEIGHT - bar_h) / 2.0;
        instances.push(lumino_gfx::CcBarInstance::new(
            bar_x,
            bar_y,
            bar_w,
            bar_h,
            [text_c.r, text_c.g, text_c.b, grab_alpha],
        ));

        // Tempo 模式：只有背景 + 手柄，没有数据柱状条
        if is_tempo {
            return instances;
        }

        // ── 非 Tempo 模式：数据柱状条 ──

        // 计算图形区域（排除 toolbar + resize handle + padding）
        // 坐标系原点为 Canvas 顶部（actual_panel_y + TOOLBAR_HEIGHT），与 Canvas 的 velocity_to_y() 一致
        // Canvas 的 bounds_height = canvas_height = panel_height - TOOLBAR_HEIGHT
        let canvas_height = panel_height - TOOLBAR_HEIGHT;
        let max_y = canvas_height; // value = 0 在 Canvas 底部，不再减去组件宽度
        let min_y = PANEL_PADDING_Y + RESIZE_HANDLE_HEIGHT; // value = 127 在 Canvas 顶部（预留 handle + padding）
        let graph_height = max_y - min_y;

        // 4. 数据柱状条（模仿 yinhe 的矩形实例化渲染）
        const BAR_WIDTH: f32 = 2.0;

        if is_velocity {
            // Velocity 模式：矩形宽度 = 音符长度（与 C# VelocityBarRenderer 一致）
            // 颜色使用与音符相同的主题色，透明度 30%
            const MIN_BAR_WIDTH: f32 = 2.0;
            const BAR_MARGIN: f32 = 1.0;
            let notes = &editor.editor_state.data.notes;

            for point in &velocity_points {
                let normalized = point.velocity as f32 / 127.0;
                let bar_h = normalized * graph_height;

                // 计算矩形 X 和宽度：从音符长度推导，带边距
                let note_x =
                    panel_x + view.keyboard_width + point.tick * view.zoom_x - view.scroll_x;
                let note_w = notes
                    .get(point.note_index)
                    .map(|n| n.length * view.zoom_x)
                    .unwrap_or(0.0);
                let bar_w = (note_w - BAR_MARGIN * 2.0).max(MIN_BAR_WIDTH);
                let bar_x = note_x + BAR_MARGIN;
                let bar_y = actual_panel_y + TOOLBAR_HEIGHT + max_y - bar_h;

                // 简单裁剪（考虑矩形宽度）
                if bar_x + bar_w < panel_x + view.keyboard_width || bar_x > panel_x + canvas.size.x
                {
                    continue;
                }

                instances.push(lumino_gfx::CcBarInstance::new(
                    bar_x,
                    bar_y,
                    bar_w,
                    bar_h,
                    bar_color_arr,
                ));
            }
        } else if is_bend {
            // Bend 模式：值范围 -8192 到 8191，中心在面板中间
            // 颜色按弯音值映射：负值冷色（蓝紫），正值暖色（橙红）
            const BEND_MAX: f32 = 8191.0;
            const BEND_MIN: f32 = -8192.0;

            for point in &bend_points {
                let normalized = (point.value as f32 - BEND_MIN) / (BEND_MAX - BEND_MIN);
                let bar_h = normalized * graph_height;
                let bar_x =
                    panel_x + view.keyboard_width + point.tick * view.zoom_x - view.scroll_x;
                let bar_y = actual_panel_y + TOOLBAR_HEIGHT + max_y - bar_h;

                // 简单裁剪
                if bar_x + BAR_WIDTH < panel_x + view.keyboard_width
                    || bar_x > panel_x + canvas.size.x
                {
                    continue;
                }

                instances.push(lumino_gfx::CcBarInstance::new(
                    bar_x,
                    bar_y,
                    BAR_WIDTH,
                    bar_h,
                    bar_color_arr,
                ));
            }
        } else {
            // CC 模式：值范围 0 到 127，颜色按 CC 值热力映射
            const MAX_VALUE: f32 = 127.0;

            for point in &cc_points {
                let normalized = point.value as f32 / MAX_VALUE;
                let bar_h = normalized * graph_height;
                let bar_x =
                    panel_x + view.keyboard_width + point.tick * view.zoom_x - view.scroll_x;
                let bar_y = actual_panel_y + TOOLBAR_HEIGHT + max_y - bar_h;

                // 简单裁剪
                if bar_x + BAR_WIDTH < panel_x + view.keyboard_width
                    || bar_x > panel_x + canvas.size.x
                {
                    continue;
                }

                instances.push(lumino_gfx::CcBarInstance::new(
                    bar_x,
                    bar_y,
                    BAR_WIDTH,
                    bar_h,
                    bar_color_arr,
                ));
            }
        }

        instances
    }

    /// 更新音符数据：主音符同步写入 + 洋葱皮异步派发
    ///
    /// Phase 1: 主音轨主音符 → 主线程同步写入双缓冲 + swap（~1ms）
    ///   → WGPU 线程立即可见，零延迟，白屏问题根治
    /// Phase 2: 洋葱皮 → 派发到 NoteWorker 异步计算，完成后二次 swap
    ///   → 50-200ms 延迟，但不阻塞主音符渲染
    pub(super) fn update_note_data_for_wgpu_thread(&mut self) {
        puffin::profile_scope!("update_note_data");

        // 走带模式：音符由 arrangement_renderer 直接绘制，跳过钢琴卷帘
        if self.root.is_arrangement_mode() {
            return;
        }

        let note_index_dirty = self.root.editor.spatial.note_index_dirty.get();
        let is_drawing = matches!(
            self.root.editor.editor_state.interaction.edit_state,
            crate::editor::EditState::Drawing { .. }
        );

        // 检测视口变化
        let v = &self.root.editor.editor_state.view;
        let canvas_size = &self.root.editor.editor_state.canvas.size;
        let current_viewport_hash = crate::host::RenderCache::compute_viewport_hash(
            v.scroll_x,
            v.scroll_y,
            v.zoom_x,
            v.zoom_y,
            canvas_size.x,
            canvas_size.y,
            v.visible_key_count,
        );
        let viewport_changed =
            current_viewport_hash != self.render_ctx.render_cache.note_viewport_hash;

        // 洋葱皮专用量化哈希（32px 取整，微小移动不触发重算）
        let current_onion_hash = crate::host::RenderCache::compute_onion_viewport_hash(
            v.scroll_x,
            v.scroll_y,
            v.zoom_x,
            v.zoom_y,
            canvas_size.x,
            canvas_size.y,
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

        // ═══ Phase 1: 主音符同步写入 ═══
        // 仅当数据变化时才重建主音符，仅视口变化时跳过（音符数据相同，减少重复上传）
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

        // ═══ Phase 2: 洋葱皮异步派发 ═══
        // 数据变化 或 量化视口变化（微小滚动不触发，避免 worker 过载）
        if note_data_changed || onion_viewport_changed {
            self.ensure_note_worker();
            if let Some(ref worker) = self.render_ctx.note_worker {
                puffin::profile_scope!("dispatch_onion_skin_job");
                let vp_logical = self.render_ctx.viewport.logical_size();
                let os_snapshot =
                    self.collect_onion_skin_snapshot((vp_logical.width, vp_logical.height));

                worker.send(super::note_worker::OnionSkinJob {
                    snapshot: os_snapshot,
                    onion_note_buffer: std::sync::Arc::clone(
                        &self.render_ctx.render_cache.onion_note_buffer,
                    ),
                    done_tx: None,
                });
                self.render_ctx.render_cache.onion_viewport_hash = current_onion_hash;
            }
        }

        if note_index_dirty {
            self.root.editor.spatial.note_index_dirty.set(false);
        }
    }

    /// 构建渲染参数
    pub(super) fn build_render_params(&self, data: super::data::RenderData) -> RenderParams {
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

        // 使用 collect_viewport_info 获取正确的 canvas_offset 和 canvas_size
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
                (es.canvas.offset.x, es.canvas.offset.y),
                (es.canvas.size.x, es.canvas.size.y),
                es.view.keyboard_width,
                es.view.ruler_height,
            )
        };

        // 走带模式：构建 arrangement uniform
        let bg_color_arr = colors.bg;
        let bar_color = colors.bar_line;
        let arrangement_uniform = if is_arrangement_mode {
            let av = &self.root.arrangement_view.viewport;
            let track_count = self.root.sidebar.tracks.len().max(1) as f32;
            let mut track_colors = [[0.0_f32; 4]; 16];
            for (i, &c) in ARRANGEMENT_PALETTE.iter().enumerate().take(16) {
                track_colors[i] = [c[0], c[1], c[2], 1.0];
            }
            ArrangementUniform {
                scroll: [data.scroll.0, data.scroll.1],
                zoom: data.zoom.0,
                track_height: av.track_height,
                notes_per_track: 128.0,
                viewport_size: [data.viewport_size.width, data.viewport_size.height],
                canvas_offset: [canvas_offset.0, canvas_offset.1],
                playhead_x: if self.root.editor.playback_position > 0.0 {
                    self.root.editor.playback_position * av.zoom_x - data.scroll.0
                } else {
                    -1.0
                },
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
                ..ArrangementUniform::default()
            }
        } else {
            ArrangementUniform::default()
        };

        // 计算力度面板矩形（用于 wgpu scissor）
        let velocity_panel_rect = if is_arrangement_mode {
            None
        } else {
            let es = &self.root.editor.editor_state;
            // velocity 面板在 grid Canvas 下方，中间隔了水平滚动条（20px）
            const H_SCROLLBAR_HEIGHT: f32 = 20.0;
            Some((
                es.canvas.offset.x,
                es.canvas.offset.y + es.canvas.size.y + H_SCROLLBAR_HEIGHT,
                es.canvas.size.x,
                self.root.velocity_panel_height + PANEL_PADDING_Y + 10.0,
            ))
        };

        RenderParams {
            viewport_size: (physical_size.width, physical_size.height),
            logical_size: (data.viewport_size.width, data.viewport_size.height),
            scale_factor: self.render_ctx.viewport.scale_factor(),
            scroll: data.scroll,
            zoom: data.zoom,
            keyboard_width,
            ruler_height,
            background_color: bg_color,
            color_bg: colors.bg,
            color_bg_black_key: colors.black_key,
            color_bar: colors.bar_line,
            color_beat: colors.beat_line,
            color_half_beat: colors.half_beat_line,
            color_grid: colors.grid_line,
            color_key_line: colors.key_line,
            grid_instances: data.grid_instances,
            note_instances: vec![],
            ruler_instances: data.ruler_instances,
            keyboard_instances: data.keyboard_instances,
            ticks_per_measure: (ppq as u32) * 4,
            ticks_per_beat: ppq as u32,
            regenerate_grid: false,
            canvas_offset: (canvas_offset.0, canvas_offset.1),
            canvas_size: (canvas_size.0, canvas_size.1),
            ppq: ppq as f32,
            max_key_index,
            is_arrangement_mode,
            arrangement_note_instances: data.arrangement_note_instances,
            arrangement_uniform,
            cc_bar_instances: data.cc_bar_instances,
            velocity_panel_rect,
        }
    }

    // build_velocity_graph_instances 已移除 — 改用 build_cc_bar_instances 统一矩形渲染
}
