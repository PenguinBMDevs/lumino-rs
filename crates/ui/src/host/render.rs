//! Host 渲染子模块 - 处理 UI 和音符渲染

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
        self.render_notes(frame, view, gfx);

        // 第二步：渲染 iced UI
        self.render_iced_ui(frame, view);
    }

    /// 渲染 iced UI 层
    fn render_iced_ui(&mut self, frame: &wgpu::SurfaceTexture, texture_view: &wgpu::TextureView) {
        // 临时取出缓存以避免借用冲突
        let cache = std::mem::take(&mut self.cache);

        // 构建视图和界面
        let root_view = self.root.view();
        let mut interface = UserInterface::build(
            root_view,
            self.viewport.logical_size(),
            cache,
            &mut self.renderer,
        );

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

    /// 使用 wgpu 渲染音符
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
                label: Some("note_render_encoder"),
            });

        // 菜单打开时，禁止更新光标与渲染预览音符
        // 以避免菜单被覆盖或产生误操作
        if !self.root.should_render_preview_note() {
            self.root.update_editor_cursor(None);
        } else {
            // 同步光标位置到编辑器
            self.root.update_editor_cursor(self.cursor_position);
        }

        // 获取需要绘制的音符实例
        let instances = self.root.get_note_instances();

        // 使用逻辑尺寸绘制音符（与 iced 坐标系保持一致）
        let logical_size = self.viewport.logical_size();

        if !instances.is_empty() {
            // 准备渲染（执行计算剔除）
            self.note_renderer.prepare(
                &mut encoder,
                &instances,
                &gfx.device,
                &gfx.queue,
                (logical_size.width, logical_size.height),
            );
        }

        // 开始渲染通道，始终清除背景
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("note_render_pass"),
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

        if !instances.is_empty() {
            // 计算 Canvas 区域的裁剪矩形（限制音符只在钢琴卷帘内显示）
            // 转换为物理像素坐标用于裁剪矩形
            let scale = self.viewport.scale_factor();
            let canvas_offset = self.root.editor.canvas_offset;
            let canvas_size = self.root.editor.canvas_size;
            let physical_size = self.viewport.physical_size();

            let scissor_x = ((canvas_offset.x * scale) as u32).min(physical_size.width);
            let scissor_y = ((canvas_offset.y * scale) as u32).min(physical_size.height);
            let scissor_width =
                ((canvas_size.x * scale) as u32).min(physical_size.width.saturating_sub(scissor_x));
            let scissor_height = ((canvas_size.y * scale) as u32)
                .min(physical_size.height.saturating_sub(scissor_y));

            if scissor_width > 0 && scissor_height > 0 {
                self.note_renderer.draw(
                    &mut render_pass,
                    true,
                    Some((scissor_x, scissor_y, scissor_width, scissor_height)),
                );
            }
        }

        // 释放 render_pass 并提交命令
        drop(render_pass);
        gfx.queue.submit(std::iter::once(encoder.finish()));
    }

    /// 清除 UI 缓存以强制重绘
    #[inline]
    pub(crate) fn clear_cache(&mut self) {
        self.cache = std::mem::take(&mut self.cache);
    }
}
