//! Host 渲染子模块 - 处理 UI、网格和音符渲染

use iced_wgpu::wgpu;
use iced_winit::runtime::user_interface::{self, UserInterface};

use iced_core::{Event, renderer, window as iced_window};

use crate::host::Host;
use crate::{message, window};

impl Host {
    /// 主渲染入口
    pub fn redraw_requested(
        &mut self,
        frame: &wgpu::SurfaceTexture,
        view: &wgpu::TextureView,
        gfx: &lumino_gfx::Context,
    ) {
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

        // 第一步：使用 wgpu 渲染音符（位于 UI 层下方）
        self.render_notes_cached(frame, view, gfx);

        // 第二步：渲染 iced UI（仅在需要时重建 UI 树）
        if !self.skip_ui_rendering {
            self.render_iced_ui(frame, view);
        }
    }

    /// 渲染 iced UI 层
    fn render_iced_ui(&mut self, frame: &wgpu::SurfaceTexture, texture_view: &wgpu::TextureView) {
        // 临时取出缓存以避免借用冲突
        let cache = std::mem::take(&mut self.cache);

        let mut interface = if self.ui_dirty {
            // UI 有状态变更，重建完整界面
            UserInterface::build(
                self.root.view(),
                self.viewport.logical_size(),
                cache,
                &mut self.renderer,
            )
        } else {
            // UI 无变更，用空事件树更新（走 iced 缓存路径）
            UserInterface::build(
                self.root.view(),
                self.viewport.logical_size(),
                cache,
                &mut self.renderer,
            )
        };

        let mut messages = Vec::new();
        let (state, _) = interface.update(
            &[Event::Window(iced_window::Event::RedrawRequested(
                std::time::Instant::now(),
            ))],
            self.cursor,
            &mut self.renderer,
            &mut self.clipboard,
            &mut messages,
        );

        // 绘制界面
        let theme = self.root.theme();
        interface.draw(
            &mut self.renderer,
            &theme,
            &renderer::Style::default(),
            self.cursor,
        );

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

    /// 使用 wgpu 渲染网格和音符
    fn render_notes(
        &mut self,
        _frame: &wgpu::SurfaceTexture,
        view: &wgpu::TextureView,
        gfx: &lumino_gfx::Context,
    ) {
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
        // 以避免菜单被覆盖或产生误操作
        if !self.root.should_render_preview_note() {
            self.root.update_editor_cursor(None);
        } else {
            // 同步光标位置到编辑器
            self.root.update_editor_cursor(self.cursor_position);
        }

        // 使用逻辑尺寸绘制（与 iced 坐标系保持一致）
        let logical_size = self.viewport.logical_size();

        // ===== 准备网格线数据 =====
        self.root
            .update_grid_line_instances(&mut self.render_cache.grid_instances);
        let grid_instances = &self.render_cache.grid_instances;
        if !grid_instances.is_empty() {
            self.grid_renderer.prepare(
                &grid_instances,
                &gfx.device,
                &gfx.queue,
                (logical_size.width, logical_size.height),
            );
        }

        let canvas_offset = self.root.editor.canvas_offset;
        self.root
            .update_note_instances(&mut self.render_cache.note_instances);
        let note_instances = &self.render_cache.note_instances;
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

        if !note_instances.is_empty() {
            self.note_renderer.prepare(
                &mut encoder,
                &note_instances,
                &gfx.device,
                &gfx.queue,
                camera,
            );
        }

        // 开始渲染通道，始终清除背景
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
        let scale = self.viewport.scale_factor();
        let canvas_offset = self.root.editor.canvas_offset;
        let canvas_size = self.root.editor.canvas_size;
        let physical_size = self.viewport.physical_size();

        let scissor_x = ((canvas_offset.x * scale) as u32).min(physical_size.width);
        let scissor_y = ((canvas_offset.y * scale) as u32).min(physical_size.height);
        let scissor_width =
            ((canvas_size.x * scale) as u32).min(physical_size.width.saturating_sub(scissor_x));
        let scissor_height =
            ((canvas_size.y * scale) as u32).min(physical_size.height.saturating_sub(scissor_y));

        let has_scissor = scissor_width > 0 && scissor_height > 0;

        // ===== 绘制网格线（在音符下方） =====
        if !grid_instances.is_empty() && has_scissor {
            render_pass.set_scissor_rect(scissor_x, scissor_y, scissor_width, scissor_height);
            self.grid_renderer
                .draw(&mut render_pass, grid_instances.len() as u32);
        }

        // ===== 绘制音符 =====
        if !note_instances.is_empty() && has_scissor {
            render_pass.set_scissor_rect(scissor_x, scissor_y, scissor_width, scissor_height);
            self.note_renderer.draw(
                &mut render_pass,
                true,
                Some((scissor_x, scissor_y, scissor_width, scissor_height)),
            );
        }

        // 释放 render_pass 并提交命令
        drop(render_pass);
        gfx.queue.submit(std::iter::once(encoder.finish()));
    }

    /// 使用缓存的渲染 - 避免重复上传数据
    fn render_notes_cached(
        &mut self,
        _frame: &wgpu::SurfaceTexture,
        view: &wgpu::TextureView,
        gfx: &lumino_gfx::Context,
    ) {
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
        let grid_instances = &self.render_cache.grid_instances;

        if grid_changed && !grid_instances.is_empty() {
            self.grid_renderer.prepare(
                grid_instances,
                &gfx.device,
                &gfx.queue,
                (logical_size.width, logical_size.height),
            );
        }

        // ===== 准备音符数据（带缓存）=====
        // 视口变化、音符增删改、编辑状态或光标位置变化都需要重新生成
        let note_index_dirty = self.root.editor.note_index_dirty.get();
        let current_edit_state = self.root.editor.edit_state.clone();
        let note_viewport_changed = current_viewport_hash != self.render_cache.note_viewport_hash;
        let note_data_dirty = note_index_dirty
            || note_viewport_changed
            || current_edit_state != self.last_edit_state
            || self.cursor_position != self.last_cursor_position
            || self.render_cache.note_instances.is_empty();

        let mut notes_instances_changed = false;
        if note_data_dirty {
            self.root
                .update_note_instances(&mut self.render_cache.note_instances);
            self.render_cache.note_viewport_hash = current_viewport_hash;
            self.last_edit_state = current_edit_state;
            self.last_cursor_position = self.cursor_position;
            notes_instances_changed = true;
        }
        let note_instances = &self.render_cache.note_instances;

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

        if notes_instances_changed && !note_instances.is_empty() {
            self.note_renderer.prepare_instances(
                &mut encoder,
                note_instances,
                &gfx.device,
                &gfx.queue,
            );
        }

        if !note_instances.is_empty() {
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
        if !grid_instances.is_empty() && has_scissor {
            render_pass.set_scissor_rect(scissor_x, scissor_y, scissor_width, scissor_height);
            self.grid_renderer
                .draw(&mut render_pass, grid_instances.len() as u32);
        }

        // 绘制音符
        if !note_instances.is_empty() && has_scissor {
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
