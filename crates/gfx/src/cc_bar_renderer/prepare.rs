//! CC 柱状条渲染器 — prepare 阶段逻辑

use super::core::{
    CcBarColors, CcBarData, CcBarInstance, CcBarRenderer, CcBarViewParams, CcBarViewportUniform,
};
use crate::automation::{AutomationViewParams, build_lane_instances};
use crate::gpu_resource_tracker;
use lumino_note_core::EditMode;

// ─── CC 面板布局常量（单一来源，各函数共用，禁止局部重定义） ────────
const PANEL_PADDING_Y: f32 = 12.0;
const RESIZE_HANDLE_HEIGHT: f32 = 5.0;
const TOOLBAR_HEIGHT: f32 = 28.0;
const H_SCROLLBAR_HEIGHT: f32 = 20.0;
/// 力度/CC 数值上限（MIDI 7-bit）
const VELOCITY_MAX: f32 = 127.0;

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
            gpu_resource_tracker::sub_buffer(&self.instance_buffer);
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
    edit_mode: &EditMode,
    view_params: &CcBarViewParams,
    data: &CcBarData<'_>,
    colors: &CcBarColors,
) -> Vec<CcBarInstance> {
    let is_tempo = matches!(edit_mode, EditMode::Tempo);
    let (is_bend, is_velocity) = match edit_mode {
        EditMode::Bend => (true, false),
        EditMode::Cc(_) => (false, false),
        EditMode::Velocity => (false, true),
        EditMode::Tempo => (false, false),
    };

    let panel_x = view_params.canvas_offset_x;
    let panel_y = view_params.canvas_offset_y + view_params.canvas_size_y;
    let actual_panel_y = panel_y + H_SCROLLBAR_HEIGHT;

    let mut instances = Vec::new();

    // 1-3. 背景 / 缩放手柄 / 拖拽指示
    push_base_overlay_instances(
        &mut instances,
        panel_x,
        actual_panel_y,
        view_params.canvas_size_x,
        view_params.panel_height,
        colors,
    );

    // Tempo mode: only background + handle, no data bars
    if is_tempo {
        return instances;
    }

    // ── Non-Tempo mode: data bars / 自动化曲线 ──
    let canvas_height = view_params.panel_height - TOOLBAR_HEIGHT;
    let max_y = canvas_height;
    let min_y = PANEL_PADDING_Y + RESIZE_HANDLE_HEIGHT;
    let graph_height = max_y - min_y;

    let automation_view = AutomationViewParams {
        panel_height: view_params.panel_height,
        pixels_per_tick: view_params.zoom_x,
        scroll_x: view_params.scroll_x,
        keyboard_width: view_params.keyboard_width,
        value_zoom: view_params.value_zoom,
        value_scroll: view_params.value_scroll,
        panel_offset_x: panel_x,
        panel_offset_y: actual_panel_y,
        toolbar_height: TOOLBAR_HEIGHT,
        line_thickness: view_params.line_thickness,
    };

    if is_velocity {
        if data.velocity_curve_style {
            push_velocity_curve_instances(
                &mut instances,
                &VelocityCurveContext {
                    panel_x,
                    actual_panel_y,
                    max_y,
                    graph_height,
                    bar_color: colors.bar_color,
                    keyboard_width: view_params.keyboard_width,
                    zoom_x: view_params.zoom_x,
                    scroll_x: view_params.scroll_x,
                    canvas_size_x: view_params.canvas_size_x,
                    velocity_points: data.velocity_points,
                    line_thickness: view_params.line_thickness,
                },
            );
        } else {
            push_velocity_bar_instances(
                &mut instances,
                &VelocityBarContext {
                    panel_x,
                    actual_panel_y,
                    toolbar_height: TOOLBAR_HEIGHT,
                    max_y,
                    graph_height,
                    bar_color: colors.bar_color,
                    keyboard_width: view_params.keyboard_width,
                    zoom_x: view_params.zoom_x,
                    scroll_x: view_params.scroll_x,
                    canvas_size_x: view_params.canvas_size_x,
                    velocity_points: data.velocity_points,
                },
            );
        }
    } else if let Some(lane) = data.automation_lane {
        // CC / Bend 曲线模式：使用 AutomationLane 生成 Step/Curve 实例与锚点。
        // 节点颜色统一使用主音轨音符蓝（AUTOMATION_NODE_COLOR），
        // 与主音轨已放置音符视觉一致。
        build_lane_instances(
            &mut instances,
            view_params.canvas_size_x,
            &automation_view,
            lane,
            crate::automation::AUTOMATION_NODE_COLOR,
            true,
        );
    } else if is_bend {
        // Bend 柱状条兼容路径（无 automation lane 时降级）
        const BEND_MAX: f32 = 8191.0;
        const BEND_MIN: f32 = -8192.0;
        let points = data
            .bend_points
            .iter()
            .map(|p| (p.tick, (p.value as f32 - BEND_MIN) / (BEND_MAX - BEND_MIN)));
        push_value_bars(
            &mut instances,
            ValueBarContext {
                points,
                panel_x,
                actual_panel_y,
                keyboard_width: view_params.keyboard_width,
                zoom_x: view_params.zoom_x,
                scroll_x: view_params.scroll_x,
                canvas_size_x: view_params.canvas_size_x,
                max_y,
                graph_height,
                bar_color: colors.bar_color,
            },
        );
    } else {
        // CC 柱状条兼容路径（无 automation lane 时降级）
        const MAX_VALUE: f32 = 127.0;
        let points = data
            .cc_points
            .iter()
            .map(|p| (p.tick, p.value as f32 / MAX_VALUE));
        push_value_bars(
            &mut instances,
            ValueBarContext {
                points,
                panel_x,
                actual_panel_y,
                keyboard_width: view_params.keyboard_width,
                zoom_x: view_params.zoom_x,
                scroll_x: view_params.scroll_x,
                canvas_size_x: view_params.canvas_size_x,
                max_y,
                graph_height,
                bar_color: colors.bar_color,
            },
        );
    }

    instances
}

/// 生成背景、缩放手柄与拖拽指示等静态覆盖层实例。
fn push_base_overlay_instances(
    instances: &mut Vec<CcBarInstance>,
    panel_x: f32,
    actual_panel_y: f32,
    canvas_size_x: f32,
    panel_height: f32,
    colors: &CcBarColors,
) {
    // 1. Background
    let bg_height = panel_height + PANEL_PADDING_Y + 10.0;
    instances.push(CcBarInstance::new(
        panel_x,
        actual_panel_y,
        canvas_size_x,
        bg_height,
        colors.bg_color,
    ));

    // 2. Resize handle (below toolbar = at canvas top)
    let handle_y = actual_panel_y + TOOLBAR_HEIGHT;
    instances.push(CcBarInstance::new(
        panel_x,
        handle_y,
        canvas_size_x,
        RESIZE_HANDLE_HEIGHT,
        colors.handle_color,
    ));

    // 3. Grab indicator
    let bar_w = 40.0;
    let bar_h = 3.0;
    let bar_x = panel_x + (canvas_size_x - bar_w) / 2.0;
    let bar_y = handle_y + (RESIZE_HANDLE_HEIGHT - bar_h) / 2.0;
    instances.push(CcBarInstance::new(
        bar_x,
        bar_y,
        bar_w,
        bar_h,
        colors.grab_color,
    ));
}

/// 力度曲线渲染上下文
struct VelocityCurveContext<'a> {
    /// 面板左上角 X 坐标
    panel_x: f32,
    /// 面板实际 Y 坐标（已考虑水平滚动条高度）
    actual_panel_y: f32,
    /// 绘图区域底部 Y 偏移（相对于面板）
    max_y: f32,
    /// 曲线可用高度
    graph_height: f32,
    /// 柱条/曲线主色
    bar_color: [f32; 4],
    /// 键盘区域宽度
    keyboard_width: f32,
    /// X 轴缩放
    zoom_x: f32,
    /// X 轴滚动偏移
    scroll_x: f32,
    /// 画布宽度
    canvas_size_x: f32,
    /// 力度点数据切片
    velocity_points: &'a [lumino_note_core::VelocityPoint],
    /// 曲线/锚点线宽
    line_thickness: f32,
}

/// 力度曲线模式：折线连接力度点并绘制锚点圆。
fn push_velocity_curve_instances(
    instances: &mut Vec<CcBarInstance>,
    ctx: &VelocityCurveContext<'_>,
) {
    /// 锚点半径（像素）。直径 = 半径 * 2，较原 3.0 增大 2 像素（6px → 8px）。
    const CURVE_ANCHOR_RADIUS: f32 = 4.0;
    /// 连线透明度（1.0 = 100% 不透明）。
    const LINE_ALPHA: f32 = 1.0;
    /// 锚点透明度（1.0 = 100% 不透明）。
    const ANCHOR_ALPHA: f32 = 1.0;

    let anchor_color = [
        ctx.bar_color[0],
        ctx.bar_color[1],
        ctx.bar_color[2],
        ANCHOR_ALPHA,
    ];
    let line_color = [
        ctx.bar_color[0],
        ctx.bar_color[1],
        ctx.bar_color[2],
        LINE_ALPHA,
    ];

    let mut sorted: Vec<&lumino_note_core::VelocityPoint> = ctx.velocity_points.iter().collect();
    sorted.sort_by(|a, b| {
        a.tick
            .partial_cmp(&b.tick)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for (i, point) in sorted.iter().enumerate() {
        let bar_x = ctx.panel_x + ctx.keyboard_width + point.tick * ctx.zoom_x - ctx.scroll_x;
        let bar_y = ctx.actual_panel_y + TOOLBAR_HEIGHT + ctx.max_y
            - (point.velocity as f32 / VELOCITY_MAX * ctx.graph_height);

        if i > 0 {
            let prev = sorted[i - 1];
            let prev_x = ctx.panel_x + ctx.keyboard_width + prev.tick * ctx.zoom_x - ctx.scroll_x;
            let prev_y = ctx.actual_panel_y + TOOLBAR_HEIGHT + ctx.max_y
                - (prev.velocity as f32 / VELOCITY_MAX * ctx.graph_height);

            // Step 水平线段
            let dx = bar_x - prev_x;
            if dx > 0.5 {
                instances.push(CcBarInstance::new(
                    prev_x,
                    prev_y,
                    dx.max(ctx.line_thickness),
                    ctx.line_thickness,
                    line_color,
                ));
            }
            // Step 垂直线段
            let dy = bar_y - prev_y;
            if dy.abs() > 0.5 {
                instances.push(CcBarInstance::new(
                    bar_x - ctx.line_thickness / 2.0,
                    prev_y.min(bar_y),
                    ctx.line_thickness,
                    dy.abs(),
                    line_color,
                ));
            }
        }

        // 锚点圆（用圆角矩形渲染）
        if bar_x >= ctx.panel_x && bar_x <= ctx.panel_x + ctx.canvas_size_x {
            instances.push(CcBarInstance::with_props(
                bar_x - CURVE_ANCHOR_RADIUS,
                bar_y - CURVE_ANCHOR_RADIUS,
                CURVE_ANCHOR_RADIUS * 2.0,
                CURVE_ANCHOR_RADIUS * 2.0,
                anchor_color,
                CURVE_ANCHOR_RADIUS,
                0.0,
            ));
        }
    }
}

/// 力度柱状图渲染上下文
struct VelocityBarContext<'a> {
    /// 面板左上角 X 坐标
    panel_x: f32,
    /// 面板实际 Y 坐标
    actual_panel_y: f32,
    /// 工具栏高度
    toolbar_height: f32,
    /// 绘图区域底部 Y 偏移
    max_y: f32,
    /// 曲线可用高度
    graph_height: f32,
    /// 柱条主色
    bar_color: [f32; 4],
    /// 键盘区域宽度
    keyboard_width: f32,
    /// X 轴缩放
    zoom_x: f32,
    /// X 轴滚动偏移
    scroll_x: f32,
    /// 画布宽度
    canvas_size_x: f32,
    /// 力度点数据切片
    velocity_points: &'a [lumino_note_core::VelocityPoint],
}

/// 力度柱状图模式：bar 宽度取 note 长度（从 VelocityPoint 直接读取）。
fn push_velocity_bar_instances(instances: &mut Vec<CcBarInstance>, ctx: &VelocityBarContext<'_>) {
    const MIN_BAR_WIDTH: f32 = 2.0;
    const BAR_MARGIN: f32 = 1.0;

    for point in ctx.velocity_points {
        let normalized = point.velocity as f32 / VELOCITY_MAX;
        let bar_h = normalized * ctx.graph_height;

        let note_x = ctx.panel_x + ctx.keyboard_width + point.tick * ctx.zoom_x - ctx.scroll_x;
        // 直接从 VelocityPoint 读取 length，避免 im::Vector::get 的 O(log n) 树查找
        let note_w = point.length * ctx.zoom_x;
        let bar_w = (note_w - BAR_MARGIN * 2.0).max(MIN_BAR_WIDTH);
        let bar_x = note_x + BAR_MARGIN;
        let bar_y = ctx.actual_panel_y + ctx.toolbar_height + ctx.max_y - bar_h;

        // Simple clipping (considering bar width)
        if bar_x + bar_w < ctx.panel_x + ctx.keyboard_width
            || bar_x > ctx.panel_x + ctx.canvas_size_x
        {
            continue;
        }

        instances.push(CcBarInstance::new(
            bar_x,
            bar_y,
            bar_w,
            bar_h,
            ctx.bar_color,
        ));
    }
}

/// 通用数值柱状条渲染上下文
struct ValueBarContext<I: IntoIterator<Item = (f32, f32)>> {
    /// 待绘制的 (tick, 归一化值) 序列
    points: I,
    /// 面板左上角 X 坐标
    panel_x: f32,
    /// 面板实际 Y 坐标
    actual_panel_y: f32,
    /// 键盘区域宽度
    keyboard_width: f32,
    /// X 轴缩放
    zoom_x: f32,
    /// X 轴滚动偏移
    scroll_x: f32,
    /// 画布宽度
    canvas_size_x: f32,
    /// 绘图区域底部 Y 偏移
    max_y: f32,
    /// 曲线可用高度
    graph_height: f32,
    /// 柱条主色
    bar_color: [f32; 4],
}

/// 通用数值柱状条（Bend / CC 降级路径）：对给定 (tick, 归一化值) 序列绘制柱条。
fn push_value_bars<I: IntoIterator<Item = (f32, f32)>>(
    instances: &mut Vec<CcBarInstance>,
    ctx: ValueBarContext<I>,
) {
    const BAR_WIDTH: f32 = 2.0;

    for (tick, normalized) in ctx.points {
        let bar_h = normalized * ctx.graph_height;
        let bar_x = ctx.panel_x + ctx.keyboard_width + tick * ctx.zoom_x - ctx.scroll_x;
        let bar_y = ctx.actual_panel_y + TOOLBAR_HEIGHT + ctx.max_y - bar_h;

        // Simple clipping
        if bar_x + BAR_WIDTH < ctx.panel_x + ctx.keyboard_width
            || bar_x > ctx.panel_x + ctx.canvas_size_x
        {
            continue;
        }

        instances.push(CcBarInstance::new(
            bar_x,
            bar_y,
            BAR_WIDTH,
            bar_h,
            ctx.bar_color,
        ));
    }
}
