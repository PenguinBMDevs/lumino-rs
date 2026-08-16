use super::super::super::super::params::RenderParams;
use super::super::types::HiResMeta;
use crate::{TextureWaterfallConfig, TextureWaterfallRenderMode, TextureWaterfallRenderer, TextureWaterfallUniform, WaterfallTileCoord};

// ── 贴图瀑布流视口驱动 ────────────────────────────────────

/// 计算单个 tile coordinate 对应的 uniform（若不可见则返回 None）
fn compute_tile_uniform(
    coord: &WaterfallTileCoord,
    time_g: u32,
    ticks_per_group: u32,
    config: &TextureWaterfallConfig,
    renderer: &TextureWaterfallRenderer,
    base_x: f32,
    area_y: f32,
    area_h: f32,
    canvas_w: f32,
    canvas_h: f32,
    zoom_x: f32,
    scroll_x: f32,
    scale: f32,
) -> Option<(WaterfallTileCoord, TextureWaterfallUniform)> {
    if !renderer.has_tile_or_dirty_overlay(coord) {
        return None;
    }

    let tick_start = time_g * ticks_per_group;
    let (area_x, area_w) = match config.render_mode {
        TextureWaterfallRenderMode::Native => {
            let texture_zoom = config.tile_width_px as f32 / ticks_per_group as f32;
            let tick_offset = scroll_x / zoom_x;
            let area_x = base_x + (tick_start as f32 - tick_offset) * texture_zoom * scale;
            let area_w = config.tile_width_px as f32 * scale;
            (area_x, area_w)
        }
        TextureWaterfallRenderMode::Stretch => {
            let area_x = base_x + (tick_start as f32 * zoom_x - scroll_x) * scale;
            let area_w = ticks_per_group as f32 * zoom_x * scale;
            (area_x, area_w)
        }
    };

    let uniform = TextureWaterfallUniform::new(area_x, area_y, area_w, area_h, canvas_w, canvas_h);
    Some((*coord, uniform))
}

/// 收集当前视口内所有可见的贴图瀑布流坐标与 uniform
fn collect_visible_hires_coords(
    renderer: &TextureWaterfallRenderer,
    config: &TextureWaterfallConfig,
    meta: &HiResMeta,
    params: &RenderParams,
) -> Vec<(WaterfallTileCoord, TextureWaterfallUniform)> {
    let scale = params.scale_factor;
    let zoom_x = params.zoom.0;
    let zoom_y = params.zoom.1;
    let ticks_per_group = meta.ticks_per_group;
    let scroll_x = params.scroll.0;
    let canvas_w_logical = params.canvas_size.0;
    let t_start = (scroll_x / zoom_x).max(0.0) as u32;
    let t_end = ((scroll_x + canvas_w_logical) / zoom_x) as u32;
    let g_start = t_start / ticks_per_group;
    let g_end = (t_end / ticks_per_group).saturating_add(1);
    let key_count = meta.key_count;

    let base_x = (params.canvas_offset.0 + params.keyboard_width) * scale;
    let scroll_y = params.scroll.1;
    let area_y = (params.canvas_offset.1 + params.ruler_height - scroll_y) * scale;
    let area_h = key_count as f32 * zoom_y * scale;
    let canvas_w = params.viewport_size.0 as f32;
    let canvas_h = params.viewport_size.1 as f32;

    let mut visible = Vec::new();
    for track_g in 0..meta.track_groups {
        for time_g in g_start..g_end {
            let coord = WaterfallTileCoord::new(track_g, time_g);
            if let Some(result) = compute_tile_uniform(
                &coord,
                time_g,
                ticks_per_group,
                config,
                renderer,
                base_x,
                area_y,
                area_h,
                canvas_w,
                canvas_h,
                zoom_x,
                scroll_x,
                scale,
            ) {
                visible.push(result);
            }
        }
    }
    visible
}

/// 贴图瀑布流视口驱动：准备 uniform
pub(crate) fn update_hires_viewport(
    renderer: &mut Option<TextureWaterfallRenderer>,
    meta: &Option<HiResMeta>,
    config: &Option<TextureWaterfallConfig>,
    params: &RenderParams,
    _device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Vec<(WaterfallTileCoord, TextureWaterfallUniform)> {
    let (Some(renderer), Some(config), Some(meta)) = (renderer, config, meta) else {
        return Vec::new();
    };
    if !config.enabled || params.is_arrangement_mode {
        return Vec::new();
    }
    if params.zoom.0 <= 0.0 || params.zoom.1 <= 0.0 {
        return Vec::new();
    }
    if meta.ticks_per_group == 0 {
        return Vec::new();
    }

    let visible = collect_visible_hires_coords(renderer, config, meta, params);
    renderer.prepare(queue, &visible);
    renderer.prepare_dirty_overlays(queue, &visible);
    visible
}
