//! CC 柱状条渲染器 — prepare 阶段逻辑

use super::core::{
    CcBarColors, CcBarData, CcBarInstance, CcBarRenderer, CcBarViewParams, CcBarViewportUniform,
};

impl CcBarRenderer {
    /// 准备渲染数据
    ///
    /// `instances` — CC 柱状条实例列表（屏幕空间坐标）
    /// `viewport_size` — 视口尺寸（用于 NDC 转换）
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        instances: &[CcBarInstance],
        viewport_size: (f32, f32),
    ) {
        puffin::profile_function!();

        let instance_count = instances.len();

        // 扩容实例缓冲区
        if instance_count > self.capacity {
            let new_capacity = (self.capacity * Self::GROWTH_FACTOR).max(instance_count);
            self.instance_buffer = Self::create_instance_buffer(device, new_capacity);
            self.capacity = new_capacity;
        }

        // 上传实例数据
        if instance_count > 0 {
            queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(instances));
        }

        // 更新视口 uniform
        let viewport_uniform = CcBarViewportUniform::new(viewport_size.0, viewport_size.1);
        queue.write_buffer(
            &self.viewport_buffer,
            0,
            bytemuck::cast_slice(&[viewport_uniform]),
        );
    }
}

/// Build CC bar instances from editor state data.
///
/// All colors are passed as parameters (UI layer extracts from theme).
/// Data points (velocity_points, cc_points, bend_points) are pre-computed
/// by the UI layer.
pub fn build_cc_bar_instances(
    edit_mode: &lumino_core::EditMode,
    view_params: &CcBarViewParams,
    data: &CcBarData<'_>,
    colors: &CcBarColors,
) -> Vec<CcBarInstance> {
    use lumino_core::EditMode;

    let is_tempo = matches!(edit_mode, EditMode::Tempo);
    let (is_bend, is_velocity) = match edit_mode {
        EditMode::Bend => (true, false),
        EditMode::Cc(_) => (false, false),
        EditMode::Velocity => (false, true),
        EditMode::Tempo => (false, false),
    };

    const PANEL_PADDING_Y: f32 = 12.0;
    const RESIZE_HANDLE_HEIGHT: f32 = 5.0;
    const TOOLBAR_HEIGHT: f32 = 28.0;
    const H_SCROLLBAR_HEIGHT: f32 = 20.0;

    let panel_x = view_params.canvas_offset_x;
    let panel_y = view_params.canvas_offset_y + view_params.canvas_size_y;
    let actual_panel_y = panel_y + H_SCROLLBAR_HEIGHT;

    let mut instances = Vec::new();

    // 1. Background
    let bg_height = view_params.panel_height + PANEL_PADDING_Y + 10.0;
    instances.push(CcBarInstance::new(
        panel_x,
        actual_panel_y,
        view_params.canvas_size_x,
        bg_height,
        colors.bg_color,
    ));

    // 2. Resize handle (below toolbar = at canvas top)
    let handle_y = actual_panel_y + TOOLBAR_HEIGHT;
    instances.push(CcBarInstance::new(
        panel_x,
        handle_y,
        view_params.canvas_size_x,
        RESIZE_HANDLE_HEIGHT,
        colors.handle_color,
    ));

    // 3. Grab indicator
    let bar_w = 40.0;
    let bar_h = 3.0;
    let bar_x = panel_x + (view_params.canvas_size_x - bar_w) / 2.0;
    let bar_y = handle_y + (RESIZE_HANDLE_HEIGHT - bar_h) / 2.0;
    instances.push(CcBarInstance::new(
        bar_x,
        bar_y,
        bar_w,
        bar_h,
        colors.grab_color,
    ));

    // Tempo mode: only background + handle, no data bars
    if is_tempo {
        return instances;
    }

    // ── Non-Tempo mode: data bars ──
    let canvas_height = view_params.panel_height - TOOLBAR_HEIGHT;
    let max_y = canvas_height;
    let min_y = PANEL_PADDING_Y + RESIZE_HANDLE_HEIGHT;
    let graph_height = max_y - min_y;

    const BAR_WIDTH: f32 = 2.0;

    if is_velocity {
        // Velocity mode: bar width = note length (matches C# VelocityBarRenderer)
        const MIN_BAR_WIDTH: f32 = 2.0;
        const BAR_MARGIN: f32 = 1.0;

        for point in data.velocity_points {
            let normalized = point.velocity as f32 / 127.0;
            let bar_h = normalized * graph_height;

            let note_x = panel_x + view_params.keyboard_width + point.tick * view_params.zoom_x
                - view_params.scroll_x;
            let note_w = data
                .notes
                .get(point.note_index)
                .map(|n| n.length * view_params.zoom_x)
                .unwrap_or(0.0);
            let bar_w = (note_w - BAR_MARGIN * 2.0).max(MIN_BAR_WIDTH);
            let bar_x = note_x + BAR_MARGIN;
            let bar_y = actual_panel_y + TOOLBAR_HEIGHT + max_y - bar_h;

            // Simple clipping (considering bar width)
            if bar_x + bar_w < panel_x + view_params.keyboard_width
                || bar_x > panel_x + view_params.canvas_size_x
            {
                continue;
            }

            instances.push(CcBarInstance::new(
                bar_x,
                bar_y,
                bar_w,
                bar_h,
                colors.bar_color,
            ));
        }
    } else if is_bend {
        // Bend mode: value range -8192..8191, center at panel middle
        const BEND_MAX: f32 = 8191.0;
        const BEND_MIN: f32 = -8192.0;

        for point in data.bend_points {
            let normalized = (point.value as f32 - BEND_MIN) / (BEND_MAX - BEND_MIN);
            let bar_h = normalized * graph_height;
            let bar_x = panel_x + view_params.keyboard_width + point.tick * view_params.zoom_x
                - view_params.scroll_x;
            let bar_y = actual_panel_y + TOOLBAR_HEIGHT + max_y - bar_h;

            // Simple clipping
            if bar_x + BAR_WIDTH < panel_x + view_params.keyboard_width
                || bar_x > panel_x + view_params.canvas_size_x
            {
                continue;
            }

            instances.push(CcBarInstance::new(
                bar_x,
                bar_y,
                BAR_WIDTH,
                bar_h,
                colors.bar_color,
            ));
        }
    } else {
        // CC mode: value range 0..127
        const MAX_VALUE: f32 = 127.0;

        for point in data.cc_points {
            let normalized = point.value as f32 / MAX_VALUE;
            let bar_h = normalized * graph_height;
            let bar_x = panel_x + view_params.keyboard_width + point.tick * view_params.zoom_x
                - view_params.scroll_x;
            let bar_y = actual_panel_y + TOOLBAR_HEIGHT + max_y - bar_h;

            // Simple clipping
            if bar_x + BAR_WIDTH < panel_x + view_params.keyboard_width
                || bar_x > panel_x + view_params.canvas_size_x
            {
                continue;
            }

            instances.push(CcBarInstance::new(
                bar_x,
                bar_y,
                BAR_WIDTH,
                bar_h,
                colors.bar_color,
            ));
        }
    }

    instances
}
