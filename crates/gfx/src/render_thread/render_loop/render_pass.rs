use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::super::params::RenderParams;
use super::super::stats::RenderStats;
use super::runner::context::{RenderContext, RenderFrameState};
use crate::{CameraParams, CameraUniform, TileCoord};

/// 执行渲染通道（含走带/钢琴卷帘/CC 柱状条）
pub fn execute_render_pass(
    encoder: &mut wgpu::CommandEncoder,
    ctx: &RenderContext,
    params: &RenderParams,
    hires_visible_coords: &[TileCoord],
    render_notes: bool,
    frame: &mut RenderFrameState,
) {
    let Some(texture) = frame.current_texture.as_ref() else {
        return;
    };

    let width = params.viewport_size.0.max(1);
    let height = params.viewport_size.1.max(1);

    // 优先使用 ensure_textures 缓存的 texture view；未命中时退化为每帧创建
    let mut fresh_view: Option<wgpu::TextureView> = None;
    let view: &wgpu::TextureView = if let Some(v) = frame.texture_view.as_ref() {
        v
    } else {
        fresh_view = Some(texture.create_view(&wgpu::TextureViewDescriptor::default()));
        match fresh_view {
            Some(ref v) => v,
            None => unreachable!(),
        }
    };

    // depth 仅在需要时存在（视频导出为纯 2D，跳过 depth attachment）
    let depth_view = frame.depth_texture_view.as_ref();

    let depth_stencil_attachment = depth_view.map(|dv| wgpu::RenderPassDepthStencilAttachment {
        view: dv,
        depth_ops: Some(wgpu::Operations {
            load: wgpu::LoadOp::Clear(1.0),
            store: wgpu::StoreOp::Discard,
        }),
        stencil_ops: None,
    });

    let clear_color = wgpu::Color {
        r: params.background_color[0],
        g: params.background_color[1],
        b: params.background_color[2],
        a: params.background_color[3],
    };

    // 工程走带模式：渲染走带背景和音符
    if params.is_arrangement_mode {
        // 排列模式同样需要 scissor rect 剔除，避免渲染视口外音符
        let arr_scissor_x = ((params.canvas_offset.0 * params.scale_factor) as u32).min(width);
        let arr_scissor_y = ((params.canvas_offset.1 * params.scale_factor) as u32).min(height);
        let arr_scissor_w = ((params.canvas_size.0 * params.scale_factor) as u32)
            .min(width.saturating_sub(arr_scissor_x));
        let arr_scissor_h = ((params.canvas_size.1 * params.scale_factor) as u32)
            .min(height.saturating_sub(arr_scissor_y));

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("arrangement_render_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear_color),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: depth_stencil_attachment.clone(),
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        render_pass.set_scissor_rect(arr_scissor_x, arr_scissor_y, arr_scissor_w, arr_scissor_h);
        // 走带渲染器绘制（背景 + 网格 + 音符 + 演奏指示线）
        frame.renderers.arrangement.draw(&mut render_pass);
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

    frame
        .renderers
        .note
        .prepare_pass(encoder, camera, &ctx.queue);

    {
        puffin::profile_scope!("render_pass");
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("offscreen_render_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear_color),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: depth_stencil_attachment.clone(),
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
        frame.renderers.grid.draw(&mut render_pass, 1);

        // 绘制高精度洋葱皮贴图（网格之上、低精度洋葱皮之下，半透明叠加）
        if let Some(hires) = frame.hires_renderer.as_ref() {
            let has_depth = depth_view.is_some();
            render_pass.set_scissor_rect(scissor_x, scissor_y, scissor_width, scissor_height);
            hires.render(&mut render_pass, hires_visible_coords, has_depth);
            // 绘制编辑后的临时脏区域覆层（在正常贴图之上，颜色与当前音轨一致）
            hires.render_dirty_overlays(&mut render_pass, hires_visible_coords, has_depth);
        }

        // 绘制音符（HiRes 贴图模式下音符已包含在贴图中，跳过）
        // 弯音编辑模式下音符仍然渲染，半透明遮罩叠加在上方
        if render_notes {
            render_pass.set_scissor_rect(scissor_x, scissor_y, scissor_width, scissor_height);
            frame.renderers.note.draw(
                &mut render_pass,
                true,
                Some((scissor_x, scissor_y, scissor_width, scissor_height)),
            );
        }

        // 弯音编辑模式：在钢琴卷帘区域叠加半透明遮罩（独立于自动化面板的力度面板 scissor）
        if params.pitch_bend_mode && !params.cc_bar_instances.is_empty() {
            render_pass.set_scissor_rect(scissor_x, scissor_y, scissor_width, scissor_height);
            frame
                .renderers
                .cc_bar
                .draw(&mut render_pass, params.cc_bar_instances.len() as u32);
        }

        // 绘制标尺
        if !params.ruler_instances.is_empty() {
            render_pass.set_scissor_rect(0, 0, width, height);
            frame
                .renderers
                .ruler
                .draw(&mut render_pass, params.ruler_instances.len() as u32);
        }

        // 绘制 CC 柱状条（力度面板 — 统一矩形渲染，覆盖所有模式）
        if let Some((vx, vy, vw, vh)) = params.velocity_panel_rect {
            let scale = params.scale_factor;
            let vscissor_x = ((vx * scale) as u32).min(width);
            let vscissor_y = ((vy * scale) as u32).min(height);
            let vscissor_w = ((vw * scale) as u32).min(width.saturating_sub(vscissor_x));
            let vscissor_h = ((vh * scale) as u32).min(height.saturating_sub(vscissor_y));

            render_pass.set_scissor_rect(vscissor_x, vscissor_y, vscissor_w, vscissor_h);
            frame
                .renderers
                .cc_bar
                .draw(&mut render_pass, params.cc_bar_instances.len() as u32);
        }
    }

    // 若本帧退化为每帧创建 view，则缓存它供后续复用
    if let Some(v) = fresh_view {
        *frame.texture_view = Some(v);
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
