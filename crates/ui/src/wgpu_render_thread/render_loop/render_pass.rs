use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use iced_wgpu::wgpu;

use super::super::params::RenderParams;
use super::super::stats::RenderStats;
use lumino_gfx::{CameraParams, CameraUniform};

/// 执行渲染通道（含走带/钢琴卷帘/洋葱皮）
#[allow(clippy::too_many_arguments)]
pub fn execute_render_pass(
    encoder: &mut wgpu::CommandEncoder,
    _device: &wgpu::Device,
    current_texture: &Option<Arc<wgpu::Texture>>,
    depth_texture_view: &Option<wgpu::TextureView>,
    params: &RenderParams,
    grid_renderer: &mut lumino_gfx::GridRenderer,
    note_renderer: &mut lumino_gfx::NoteRenderer,
    ruler_renderer: &mut lumino_gfx::RulerRenderer,
    arrangement_renderer: &mut lumino_gfx::ArrangementRenderer,
    queue: &wgpu::Queue,
    onion_renderer: &mut lumino_gfx::OnionRenderer,
) {
    let (Some(texture), Some(depth_view)) = (current_texture, depth_texture_view) else {
        return;
    };

    let width = params.viewport_size.0.max(1);
    let height = params.viewport_size.1.max(1);

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    let clear_color = wgpu::Color {
        r: params.background_color[0],
        g: params.background_color[1],
        b: params.background_color[2],
        a: params.background_color[3],
    };

    // 工程走带模式：渲染走带背景和音符
    if params.is_arrangement_mode {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("arrangement_render_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear_color),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        // 走带渲染器绘制（背景 + 网格 + 音符 + 演奏指示线）
        arrangement_renderer.draw(&mut render_pass);
        return;
    }

    // 钢琴卷帘模式：正常渲染
    let camera = CameraUniform::new(CameraParams {
        scroll: [params.scroll.0, params.scroll.1],
        zoom: [params.zoom.0, params.zoom.1],
        viewport: [params.logical_size.0, params.logical_size.1],
        offset: [params.canvas_offset.0, params.canvas_offset.1],
        keyboard_width: params.keyboard_width,
        ruler_height: params.ruler_height,
        max_key_index: params.max_key_index,
    });

    note_renderer.prepare_pass(encoder, camera, queue);

    {
        puffin::profile_scope!("render_pass");
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("offscreen_render_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear_color),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        // 计算裁剪区域
        let scale = params.scale_factor;
        let scissor_x = ((params.canvas_offset.0 * scale) as u32).min(width);
        let scissor_y = ((params.canvas_offset.1 * scale) as u32).min(height);
        let scissor_width =
            ((params.canvas_size.0 * scale) as u32).min(width.saturating_sub(scissor_x));
        let scissor_height =
            ((params.canvas_size.1 * scale) as u32).min(height.saturating_sub(scissor_y));

        // 绘制背景网格
        render_pass.set_scissor_rect(scissor_x, scissor_y, scissor_width, scissor_height);
        grid_renderer.draw(&mut render_pass, 1);

        // 绘制洋葱皮背景
        render_pass.set_scissor_rect(scissor_x, scissor_y, scissor_width, scissor_height);
        onion_renderer.draw(&mut render_pass);

        // 绘制音符
        render_pass.set_scissor_rect(scissor_x, scissor_y, scissor_width, scissor_height);
        note_renderer.draw(
            &mut render_pass,
            true,
            Some((scissor_x, scissor_y, scissor_width, scissor_height)),
        );

        // 绘制标尺
        if !params.ruler_instances.is_empty() {
            render_pass.set_scissor_rect(0, 0, width, height);
            ruler_renderer.draw(&mut render_pass, params.ruler_instances.len() as u32);
        }
    }
}

/// 更新渲染统计
pub fn update_stats(
    frame_count: &mut u64,
    fps_update_time: &mut Instant,
    frame_time: Duration,
    params: &RenderParams,
    stats_clone: &Arc<Mutex<RenderStats>>,
) {
    *frame_count += 1;

    if let Ok(mut stats) = stats_clone.lock() {
        stats.frame_count = *frame_count;
        stats.last_frame_time_ms = frame_time.as_secs_f64() * 1000.0;
        stats.grid_line_count = params.grid_instances.len();
        stats.ruler_tick_count = params.ruler_instances.len();
    }

    // 更新 FPS
    if fps_update_time.elapsed().as_secs() >= 1 {
        if let Ok(mut stats) = stats_clone.lock() {
            stats.average_fps = *frame_count as f64 / fps_update_time.elapsed().as_secs_f64();
        }
        *frame_count = 0;
        *fps_update_time = Instant::now();
    }
}
