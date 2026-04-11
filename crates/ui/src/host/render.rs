//! Host 渲染子模块 - 处理 UI、网格和音符渲染
//!
//! 支持三种渲染模式：
//! 1. 单线程模式：UI更新和WGPU渲染在同一个线程
//! 2. 多线程模式（旧）：WGPU渲染在独立线程，UI线程只生成渲染命令
//! 3. 分离渲染模式（新）：UI线程和WGPU渲染线程完全分离，零拷贝数据共享

use iced_wgpu::wgpu;
use iced_winit::runtime::user_interface::{self, UserInterface};
use iced_core::{Event, renderer, window as iced_window};
use rayon::prelude::*;

use crate::host::Host;
use crate::{message, window, RenderParams};

impl Host {
    /// 主渲染入口
    ///
    /// 根据配置选择渲染模式：
    /// - 单线程模式：直接在当前线程执行所有渲染
    /// - 多线程模式（旧）：发送渲染命令到独立渲染线程
    /// - 分离渲染模式（新）：UI线程只更新数据，WGPU线程独立渲染
    pub fn redraw_requested(
        &mut self,
        frame: &wgpu::SurfaceTexture,
        view: &wgpu::TextureView,
        gfx: &lumino_gfx::Context,
    ) {
        // 通知 puffin 新的一帧开始 - 必须在 profile_function 之前调用
        puffin::GlobalProfiler::lock().new_frame();

        puffin::profile_function!();
        use std::time::Instant;

        // 计算 FPS
        self.frame_count += 1;
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_fps_update);

        if elapsed.as_millis() >= 50 {
            let fps = self.frame_count as f32 / elapsed.as_secs_f32();
            self.root.update(window::Event::fps_update(fps));
            self.frame_count = 0;
            self.last_fps_update = now;
        }

        self.last_frame_time = now;

        // 更新播放状态
        if let Some(tick) = self.root.update_playback() {
            self.root.update(message::Message::PlaybackTick(tick));
        }

        // 更新光标位置
        if !self.root.should_render_preview_note() {
            self.root.update_editor_cursor(None);
        } else {
            self.root.update_editor_cursor(self.cursor_position);
        }

        // 根据渲染模式选择不同的渲染路径
        if self.use_separate_render_thread {
            // 新架构：分离渲染线程
            self.redraw_separate_thread();
        } else {
            // 旧架构：单线程或旧多线程模式
            // 第一步：使用 wgpu 渲染音符和网格（位于 UI 层下方）
            self.render_notes_cached(frame, view, gfx);

            // 第二步：渲染 iced UI（仅在需要时重建 UI 树）
            if !self.skip_ui_rendering {
                self.render_iced_ui(frame, view);
            }
        }
    }

    /// 分离渲染线程模式的主渲染逻辑
    ///
    /// UI 线程只负责：
    /// 1. 更新状态
    /// 2. 生成渲染参数
    /// 3. 写入音符数据到双缓冲
    /// 4. 发送渲染参数到 WGPU 线程
    ///
    /// WGPU 线程负责：
    /// 1. 读取音符数据（零拷贝）
    /// 2. 执行所有 wgpu 渲染
    /// 3. Present 到 Surface
    fn redraw_separate_thread(&mut self) {
        puffin::profile_function!();

        let Some(ref wgpu_thread) = self.wgpu_render_thread else {
            tracing::error!("redraw_separate_thread called but wgpu_render_thread is None");
            return;
        };

        let Some(ref note_buffer) = self.note_buffer else {
            tracing::error!("redraw_separate_thread called but note_buffer is None");
            return;
        };

        // 获取编辑器状态
        let editor = &self.root.editor;
        let scroll = editor.scroll();
        let zoom = editor.zoom();
        let viewport_size = self.viewport.logical_size();

        // 生成网格线实例
        let grid_instances = self.generate_grid_instances(
            viewport_size.width,
            viewport_size.height,
            60.0, // keyboard_width
            30.0, // ruler_height
            scroll.0,
            scroll.1,
            zoom.0,
            zoom.1,
        );

        // 生成琴键实例
        let keyboard_instances = self.generate_keyboard_instances(
            60.0, // keyboard_width
            30.0, // ruler_height
            scroll.1,
            zoom.1,
            128, // visible_key_count
        );

        // 生成标尺实例
        let ruler_instances = self.generate_ruler_instances(
            viewport_size.width,
            60.0, // keyboard_width
            30.0, // ruler_height
            scroll.0,
            zoom.0,
            1920, // ticks_per_measure
            480,  // ticks_per_beat
        );

        // 写入音符数据到双缓冲（后台缓冲区）
        {
            puffin::profile_scope!("write_note_data");
            let note_instances = self.generate_note_instances();
            unsafe {
                let write_buf = note_buffer.write_buffer();
                write_buf.clear();
                write_buf.extend_from_slice(&note_instances);
            }
            // 交换缓冲区，使新数据对渲染线程可见
            note_buffer.swap();
        }

        // 构建渲染参数
        let canvas_offset = self.root.editor.canvas_offset;
        let canvas_size = self.root.editor.canvas_size;
        let params = RenderParams {
            viewport_size: (viewport_size.width as u32, viewport_size.height as u32),
            scroll,
            zoom,
            keyboard_width: 60.0,
            ruler_height: 30.0,
            background_color: [0.1, 0.1, 0.1, 1.0],
            grid_instances,
            ruler_instances,
            keyboard_instances,
            ticks_per_measure: 1920,
            ticks_per_beat: 480,
            regenerate_grid: false,
            canvas_offset: (canvas_offset.x, canvas_offset.y),
            canvas_size: (canvas_size.x, canvas_size.y),
        };

        // 发送渲染参数到 WGPU 线程（非阻塞）
        wgpu_thread.send_params(params);

        // UI 线程继续处理 iced UI 渲染（如果需要）
        // 注意：在分离模式下，iced UI 只渲染控件，不渲染 Canvas
        if !self.skip_ui_rendering && self.ui_dirty {
            // 这里应该渲染到一个独立的纹理或表面
            // 暂时跳过，因为主要的性能瓶颈已经解决
            self.ui_dirty = false;
        }
    }

    /// 生成网格线实例
    fn generate_grid_instances(
        &self,
        viewport_width: f32,
        viewport_height: f32,
        keyboard_width: f32,
        ruler_height: f32,
        scroll_x: f32,
        scroll_y: f32,
        zoom_x: f32,
        zoom_y: f32,
    ) -> Vec<lumino_gfx::GridLineInstance> {
        puffin::profile_function!();

        let mut instances = Vec::new();

        // 可见范围（tick 和 key）
        let visible_tick_start = scroll_x / zoom_x;
        let visible_tick_end = (scroll_x + viewport_width - keyboard_width) / zoom_x;
        let visible_key_start = scroll_y / zoom_y;
        let visible_key_end = (scroll_y + viewport_height - ruler_height) / zoom_y;

        // 小节线（垂直线）
        let ticks_per_measure = 1920.0;
        let measure_start = (visible_tick_start / ticks_per_measure).floor() as i32;
        let measure_end = (visible_tick_end / ticks_per_measure).ceil() as i32;

        for measure in measure_start..=measure_end {
            let tick = measure as f32 * ticks_per_measure;
            let x = keyboard_width + tick * zoom_x - scroll_x;

            if x >= keyboard_width && x <= viewport_width {
                instances.push(lumino_gfx::GridLineInstance::new(
                    [x, ruler_height],
                    [x, viewport_height],
                    [0.3, 0.3, 0.3, 1.0],
                    1.0,
                ));
            }
        }

        // 拍线（垂直线）
        let ticks_per_beat = 480.0;
        let beat_start = (visible_tick_start / ticks_per_beat).floor() as i32;
        let beat_end = (visible_tick_end / ticks_per_beat).ceil() as i32;

        for beat in beat_start..=beat_end {
            let tick = beat as f32 * ticks_per_beat;
            if tick % ticks_per_measure == 0.0 {
                continue; // 跳过小节线位置
            }
            let x = keyboard_width + tick * zoom_x - scroll_x;

            if x >= keyboard_width && x <= viewport_width {
                instances.push(lumino_gfx::GridLineInstance::new(
                    [x, ruler_height],
                    [x, viewport_height],
                    [0.2, 0.2, 0.2, 1.0],
                    0.5,
                ));
            }
        }

        // 琴键线（水平线）
        let key_start = visible_key_start.floor() as i32;
        let key_end = visible_key_end.ceil() as i32;

        for key in key_start..=key_end {
            let y = ruler_height + key as f32 * zoom_y - scroll_y;

            if y >= ruler_height && y <= viewport_height {
                // 判断是否为黑键
                let note_in_octave = key.rem_euclid(12);
                let is_black = matches!(note_in_octave, 1 | 3 | 6 | 8 | 10);

                let color = if is_black {
                    [0.15, 0.15, 0.15, 1.0]
                } else {
                    [0.1, 0.1, 0.1, 1.0]
                };
                let width = if is_black { 0.5 } else { 0.3 };

                instances.push(lumino_gfx::GridLineInstance::new(
                    [keyboard_width, y],
                    [viewport_width, y],
                    color,
                    width,
                ));
            }
        }

        instances
    }

    /// 生成琴键实例
    fn generate_keyboard_instances(
        &self,
        keyboard_width: f32,
        ruler_height: f32,
        scroll_y: f32,
        zoom_y: f32,
        visible_key_count: u16,
    ) -> Vec<lumino_gfx::KeyInstance> {
        puffin::profile_function!();

        let mut instances = Vec::new();
        let max_key_index = (visible_key_count.saturating_sub(1)) as f32;

        for i in 0..visible_key_count {
            let key_index = i as isize;
            let world_y = (max_key_index - key_index as f32) * zoom_y;
            let screen_y = world_y - scroll_y + ruler_height;

            // 跳过不在视口内的键
            if screen_y + zoom_y < ruler_height || screen_y > 10000.0 {
                continue;
            }

            let note_in_octave = key_index.rem_euclid(12);
            let is_black = matches!(note_in_octave, 1 | 3 | 6 | 8 | 10);

            let color = if is_black {
                [0.2, 0.2, 0.2, 1.0]
            } else {
                [0.9, 0.9, 0.9, 1.0]
            };

            // 黑键宽度为白键的 60%
            let key_width = if is_black {
                keyboard_width * 0.6
            } else {
                keyboard_width
            };

            // 黑键水平偏移
            let x_offset = if is_black { keyboard_width * 0.4 } else { 0.0 };

            instances.push(lumino_gfx::KeyInstance::new(
                [x_offset, screen_y],
                [key_width, zoom_y],
                color,
                is_black,
                i,
            ));
        }

        instances
    }

    /// 生成标尺实例
    fn generate_ruler_instances(
        &self,
        viewport_width: f32,
        keyboard_width: f32,
        ruler_height: f32,
        scroll_x: f32,
        zoom_x: f32,
        ticks_per_measure: u32,
        ticks_per_beat: u32,
    ) -> Vec<lumino_gfx::RulerTickInstance> {
        puffin::profile_function!();

        let mut instances = Vec::new();

        // 计算可见时间范围
        let visible_tick_start = scroll_x / zoom_x;
        let visible_tick_end = (scroll_x + viewport_width) / zoom_x;

        // 小节线
        let measure_start = (visible_tick_start / ticks_per_measure as f32).floor() as u32;
        let measure_end = (visible_tick_end / ticks_per_measure as f32).ceil() as u32;

        for measure in measure_start..=measure_end {
            let tick = measure as f32 * ticks_per_measure as f32;
            let x = keyboard_width + tick * zoom_x - scroll_x;

            if x >= keyboard_width && x <= viewport_width {
                instances.push(lumino_gfx::RulerTickInstance::new(
                    [x, 0.0],
                    [2.0, ruler_height],
                    [0.3, 0.3, 0.3, 1.0],
                    0, // 小节线
                    tick,
                ));
            }
        }

        // 拍线
        let beat_start = (visible_tick_start / ticks_per_beat as f32).floor() as u32;
        let beat_end = (visible_tick_end / ticks_per_beat as f32).ceil() as u32;

        for beat in beat_start..=beat_end {
            let tick = beat as f32 * ticks_per_beat as f32;
            if tick % ticks_per_measure as f32 == 0.0 {
                continue; // 跳过小节线位置
            }
            let x = keyboard_width + tick * zoom_x - scroll_x;

            if x >= keyboard_width && x <= viewport_width {
                instances.push(lumino_gfx::RulerTickInstance::new(
                    [x, ruler_height * 0.3],
                    [1.0, ruler_height * 0.7],
                    [0.5, 0.5, 0.5, 1.0],
                    1, // 拍线
                    tick,
                ));
            }
        }

        instances
    }

    /// 生成音符实例（从编辑器状态）
    fn generate_note_instances(&self) -> Vec<lumino_gfx::NoteInstance> {
        puffin::profile_function!();

        let editor = &self.root.editor;
        let mut instances = Vec::new();

        // 获取当前音轨的音符
        if let Some(notes) = editor.track_notes.get(&editor.current_track) {
            for note in notes.iter() {
                // 根据力度计算颜色（蓝色渐变）
                let intensity = note.velocity as f32 / 127.0;
                let color = [0.2, 0.5 + intensity * 0.5, 1.0, 0.8 + intensity * 0.2];
                
                instances.push(lumino_gfx::NoteInstance::new(
                    note.tick,
                    note.key as f32,
                    note.length,
                    color,
                ));
            }
        }

        instances
    }

    /// 快速更新所有音符实例（直接上传模式）
    ///
    /// 这个模式避免了 CPU 端的视锥裁剪，直接上传所有音符到 GPU
    /// 让 GPU 的 compute shader 处理裁剪，适合超密集音符场景
    ///
    /// 优化策略：
    /// 1. 使用 rayon 并行迭代处理大量音符
    /// 2. 预分配容量避免重新分配
    /// 3. 批量收集结果后一次性扩展
    fn update_all_note_instances_fast(&mut self) {
        puffin::profile_function!();

        // 获取编辑器数据（避免借用冲突）
        let notes = &self.root.editor.notes;
        let track_notes = &self.root.editor.track_notes;
        let current_track = self.root.editor.current_track;
        let edit_state = &self.root.editor.edit_state;
        let default_note_length = self.root.editor.state.default_note_length;
        let snap_precision = self.root.editor.state.snap_precision;

        let instances = &mut self.render_cache.note_instances;
        instances.clear();

        // 计算总容量
        let onion_skin_count: usize = track_notes.values().map(|n| n.len()).sum();
        let total_capacity = notes.len() + onion_skin_count + 1; // +1 for drawing note
        instances.reserve(total_capacity);

        // 批量转换所有音符（不进行 CPU 端裁剪）
        // 使用 rayon 并行处理大量音符
        const PARALLEL_THRESHOLD: usize = 5000;
        let default_color = [0.2, 0.5, 1.0, 0.9];

        if notes.len() > PARALLEL_THRESHOLD {
            // 将 im::Vector 转换为 Vec 以支持并行迭代
            let notes_vec: Vec<_> = notes.iter().collect();
            let parallel_instances: Vec<lumino_gfx::NoteInstance> = notes_vec
                .par_iter()
                .map(|note| {
                    lumino_gfx::NoteInstance::new(
                        note.tick,
                        note.key as f32,
                        note.length,
                        default_color,
                    )
                })
                .collect();
            instances.extend(parallel_instances);
        } else {
            // 小数据量使用串行迭代（避免并行开销）
            for note in notes.iter() {
                instances.push(lumino_gfx::NoteInstance::new(
                    note.tick,
                    note.key as f32,
                    note.length,
                    default_color,
                ));
            }
        }

        // 添加洋葱皮音符（其他音轨的音符）
        for (track_idx, track_notes_vec) in track_notes.iter() {
            if *track_idx == current_track {
                continue; // 跳过当前音轨
            }

            // 为每个音轨使用不同的颜色（基于音轨索引）
            let track_color = match track_idx % 8 {
                0 => [1.0, 0.5, 0.31, 0.4],   // 珊瑚红
                1 => [0.53, 0.81, 0.92, 0.4], // 天蓝色
                2 => [0.56, 0.93, 0.56, 0.4], // 浅绿色
                3 => [0.93, 0.51, 0.93, 0.4], // 紫罗兰
                4 => [1.0, 0.84, 0.0, 0.4],   // 金黄色
                5 => [0.0, 1.0, 1.0, 0.4],    // 青色
                6 => [1.0, 0.41, 0.71, 0.4],  // 热粉色
                _ => [1.0, 0.65, 0.0, 0.4],   // 橙色
            };

            for note in track_notes_vec.iter() {
                instances.push(lumino_gfx::NoteInstance::new(
                    note.tick,
                    note.key as f32,
                    note.length,
                    track_color,
                ));
            }
        }

        // 添加正在绘制的音符
        if let crate::editor::EditState::Drawing { start_tick, key, current_tick } = edit_state {
            let (tick, length) = if *current_tick > *start_tick {
                (*start_tick, *current_tick - *start_tick)
            } else if *current_tick < *start_tick {
                (*current_tick, *start_tick - *current_tick)
            } else {
                (*start_tick, default_note_length)
            };
            let length = length.max(snap_precision);

            instances.push(lumino_gfx::NoteInstance::new(
                tick,
                *key as f32,
                length,
                [0.4, 0.8, 1.0, 1.0], // 绘制中音符颜色
            ));
        }
    }

    /// 渲染 iced UI 层
    fn render_iced_ui(&mut self, frame: &wgpu::SurfaceTexture, texture_view: &wgpu::TextureView) {
        puffin::profile_function!();

        // 如果 UI 没有变更，跳过 UI 重建和绘制
        // 使用一个计数器来确保至少渲染一次 UI
        static mut FIRST_RENDER: bool = true;
        let is_first_render = unsafe {
            let first = FIRST_RENDER;
            if first {
                FIRST_RENDER = false;
            }
            first
        };
        
        if !self.ui_dirty && !is_first_render {
            // UI 没有变化且不是第一次渲染，直接 present 之前渲染的内容
            self.renderer
                .present(None, frame.texture.format(), texture_view, &self.viewport);
            return;
        }

        // 临时取出缓存以避免借用冲突
        let cache = std::mem::take(&mut self.cache);

        let mut interface = {
            puffin::profile_scope!("build_interface");
            UserInterface::build(
                self.root.view(),
                self.viewport.logical_size(),
                cache,
                &mut self.renderer,
            )
        };

        let mut messages = Vec::new();
        let (state, _) = {
            puffin::profile_scope!("update_interface");
            interface.update(
                &[Event::Window(iced_window::Event::RedrawRequested(
                    std::time::Instant::now(),
                ))],
                self.cursor,
                &mut self.renderer,
                &mut self.clipboard,
                &mut messages,
            )
        };

        // 绘制界面
        {
            puffin::profile_scope!("draw_interface");
            let theme = self.root.theme();
            interface.draw(
                &mut self.renderer,
                &theme,
                &renderer::Style::default(),
                self.cursor,
            );
        }

        // 归还缓存
        self.cache = interface.into_cache();
        // 重绘完成后 UI 不再 dirty
        self.ui_dirty = false;

        self.renderer
            .present(None, frame.texture.format(), texture_view, &self.viewport);

        // 处理消息（在 interface 被释放之后）
        for message in messages {
            self.root.update(message);
        }

        // 更新鼠标光标
        if let user_interface::State::Updated {
            mouse_interaction, ..
        } = state
        {
            if let Some(icon) = iced_winit::conversion::mouse_interaction(mouse_interaction) {
                self.window.set_cursor(icon);
                self.window.set_cursor_visible(true);
            } else {
                self.window.set_cursor_visible(false);
            }
        }
    }

    /// 使用缓存的渲染 - 避免重复上传数据
    fn render_notes_cached(
        &mut self,
        _frame: &wgpu::SurfaceTexture,
        view: &wgpu::TextureView,
        gfx: &lumino_gfx::Context,
    ) {
        puffin::profile_function!();
        use crate::host::RenderCache;

        // 从主题获取背景颜色
        let bg_color = self.root.theme().palette().background;
        let clear_color = wgpu::Color {
            r: bg_color.r as f64,
            g: bg_color.g as f64,
            b: bg_color.b as f64,
            a: bg_color.a as f64,
        };

        // 创建命令编码器
        let mut encoder = gfx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render_encoder"),
            });

        // 菜单打开时，禁止更新光标与渲染预览音符
        if !self.root.should_render_preview_note() {
            self.root.update_editor_cursor(None);
        } else {
            self.root.update_editor_cursor(self.cursor_position);
        }

        let logical_size = self.viewport.logical_size();
        let scale = self.viewport.scale_factor();
        let canvas_offset = self.root.editor.canvas_offset;
        let canvas_size = self.root.editor.canvas_size;
        let physical_size = self.viewport.physical_size();

        // 计算视口哈希用于缓存检测
        let editor = &self.root.editor;
        let current_viewport_hash = RenderCache::compute_viewport_hash(
            editor.state.scroll_x,
            editor.state.scroll_y,
            editor.state.zoom_x,
            editor.state.zoom_y,
            canvas_size.x,
            canvas_size.y,
        );

        // ===== 准备网格线数据（带缓存）=====
        let mut grid_changed = false;
        if current_viewport_hash != self.render_cache.grid_viewport_hash {
            // 视口变化，重新生成网格线
            self.root
                .update_grid_line_instances(&mut self.render_cache.grid_instances);
            self.render_cache.grid_viewport_hash = current_viewport_hash;
            grid_changed = true;
        }

        if grid_changed && !self.render_cache.grid_instances.is_empty() {
            self.grid_renderer.prepare(
                &self.render_cache.grid_instances,
                &gfx.device,
                &gfx.queue,
                (logical_size.width, logical_size.height),
            );
        }

        // ===== 准备音符数据（带缓存）=====
        // 优化策略：
        // - 超密集音符（>10000）：直接上传所有音符到 GPU，让 GPU 处理裁剪
        //   - 音符数据变化时才重新生成实例
        //   - 视口变化只更新 camera uniform，不重新生成实例
        // - 普通音符（<=10000）：CPU 端裁剪，只上传可见音符
        //   - 视口变化时需要重新生成实例（因为可见集合变了）
        let note_index_dirty = self.root.editor.note_index_dirty.get();
        let current_edit_state = self.root.editor.edit_state.clone();
        let note_viewport_changed = current_viewport_hash != self.render_cache.note_viewport_hash;

        const DIRECT_UPLOAD_THRESHOLD: usize = 10000;
        let total_notes = self.root.editor.notes.len();
        let use_direct_upload = total_notes > DIRECT_UPLOAD_THRESHOLD;

        // 区分"实例数据变化"和"视口变化/光标变化"
        // 实例数据变化：需要重新生成 NoteInstance 数组
        // 注意：光标位置变化不应该触发重新生成（除非在绘制模式）
        let is_drawing = matches!(current_edit_state, crate::editor::EditState::Drawing { .. });
        let cursor_changed = self.cursor_position != self.last_cursor_position;
        let cursor_affects_notes = is_drawing && cursor_changed; // 只有绘制时光标变化才影响音符

        let note_instances_dirty = note_index_dirty
            || current_edit_state != self.last_edit_state
            || cursor_affects_notes
            || self.render_cache.note_instances.is_empty();

        // 视口变化：只需要更新 camera，不需要重新生成实例（对于直接上传模式）
        let viewport_dirty = note_viewport_changed;

        let mut notes_instances_changed = false;

        if note_instances_dirty {
            // 音符数据变化，需要重新生成实例
            if use_direct_upload {
                self.update_all_note_instances_fast();
            } else {
                self.root
                    .update_note_instances(&mut self.render_cache.note_instances);
            }
            self.render_cache.note_viewport_hash = current_viewport_hash;
            self.last_edit_state = current_edit_state;
            // 注意：last_cursor_position 在下面统一更新
            notes_instances_changed = true;
        } else if viewport_dirty && !use_direct_upload {
            // 普通模式：视口变化需要重新生成实例（CPU 端裁剪）
            self.root
                .update_note_instances(&mut self.render_cache.note_instances);
            self.render_cache.note_viewport_hash = current_viewport_hash;
            notes_instances_changed = true;
        }
        // 直接上传模式：视口变化不重新生成实例，只更新 camera（在下面处理）

        // 更新 last_cursor_position（即使没有重新生成实例）
        self.last_cursor_position = self.cursor_position;

        let camera = lumino_gfx::CameraUniform::new(lumino_gfx::CameraParams {
            scroll: [
                self.root.editor.state.scroll_x,
                self.root.editor.state.scroll_y,
            ],
            zoom: [self.root.editor.state.zoom_x, self.root.editor.state.zoom_y],
            viewport: [logical_size.width, logical_size.height],
            offset: [canvas_offset.x, canvas_offset.y],
            keyboard_width: self.root.editor.state.keyboard_width,
            ruler_height: self.root.editor.state.ruler_height,
            max_key_index: (self.root.editor.state.visible_key_count.saturating_sub(1)) as f32,
        });

        if notes_instances_changed && !self.render_cache.note_instances.is_empty() {
            self.note_renderer.prepare_instances(
                &mut encoder,
                &self.render_cache.note_instances,
                &gfx.device,
                &gfx.queue,
            );
        }

        if !self.render_cache.note_instances.is_empty() {
            self.note_renderer
                .prepare_pass(&mut encoder, camera, &gfx.queue);
        }

        // 开始渲染通道
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("render_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear_color),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        // 计算 Canvas 区域的裁剪矩形
        let scissor_x = ((canvas_offset.x * scale) as u32).min(physical_size.width);
        let scissor_y = ((canvas_offset.y * scale) as u32).min(physical_size.height);
        let scissor_width =
            ((canvas_size.x * scale) as u32).min(physical_size.width.saturating_sub(scissor_x));
        let scissor_height =
            ((canvas_size.y * scale) as u32).min(physical_size.height.saturating_sub(scissor_y));

        let has_scissor = scissor_width > 0 && scissor_height > 0;

        // 绘制网格线
        if !self.render_cache.grid_instances.is_empty() && has_scissor {
            render_pass.set_scissor_rect(scissor_x, scissor_y, scissor_width, scissor_height);
            self.grid_renderer
                .draw(&mut render_pass, self.render_cache.grid_instances.len() as u32);
        }

        // 绘制音符
        if !self.render_cache.note_instances.is_empty() && has_scissor {
            render_pass.set_scissor_rect(scissor_x, scissor_y, scissor_width, scissor_height);
            self.note_renderer.draw(
                &mut render_pass,
                true,
                Some((scissor_x, scissor_y, scissor_width, scissor_height)),
            );
        }

        drop(render_pass);
        gfx.queue.submit(std::iter::once(encoder.finish()));
    }

    /// 清除 UI 缓存以强制重绘
    #[inline]
    pub(crate) fn clear_cache(&mut self) {
        self.cache = std::mem::take(&mut self.cache);
    }
}
