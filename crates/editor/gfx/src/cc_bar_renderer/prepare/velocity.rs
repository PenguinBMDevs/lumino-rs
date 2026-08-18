//! 力度曲线 / 柱状图 / 通用数值柱状条实例生成

use super::super::core::CcBarInstance;
use super::TOOLBAR_HEIGHT;

/// 力度/CC 数值上限（MIDI 7-bit）
const VELOCITY_MAX: f32 = 127.0;

/// 力度曲线渲染上下文
pub(crate) struct VelocityCurveContext<'a> {
    /// 面板左上角 X 坐标
    pub(crate) panel_x: f32,
    /// 面板实际 Y 坐标（已考虑水平滚动条高度）
    pub(crate) actual_panel_y: f32,
    /// 绘图区域底部 Y 偏移（相对于面板）
    pub(crate) max_y: f32,
    /// 曲线可用高度
    pub(crate) graph_height: f32,
    /// 柱条/曲线主色
    pub(crate) bar_color: [f32; 4],
    /// 键盘区域宽度
    pub(crate) keyboard_width: f32,
    /// X 轴缩放
    pub(crate) zoom_x: f32,
    /// X 轴滚动偏移
    pub(crate) scroll_x: f32,
    /// 画布宽度
    pub(crate) canvas_size_x: f32,
    /// 力度点数据切片
    pub(crate) velocity_points: &'a [lumino_note_core::VelocityPoint],
    /// 曲线/锚点线宽
    pub(crate) line_thickness: f32,
}

/// 力度曲线模式：折线连接力度点并绘制锚点圆。
pub(crate) fn push_velocity_curve_instances(
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
pub(crate) struct VelocityBarContext<'a> {
    /// 面板左上角 X 坐标
    pub(crate) panel_x: f32,
    /// 面板实际 Y 坐标
    pub(crate) actual_panel_y: f32,
    /// 工具栏高度
    pub(crate) toolbar_height: f32,
    /// 绘图区域底部 Y 偏移
    pub(crate) max_y: f32,
    /// 曲线可用高度
    pub(crate) graph_height: f32,
    /// 柱条主色
    pub(crate) bar_color: [f32; 4],
    /// 键盘区域宽度
    pub(crate) keyboard_width: f32,
    /// X 轴缩放
    pub(crate) zoom_x: f32,
    /// X 轴滚动偏移
    pub(crate) scroll_x: f32,
    /// 画布宽度
    pub(crate) canvas_size_x: f32,
    /// 力度点数据切片
    pub(crate) velocity_points: &'a [lumino_note_core::VelocityPoint],
}

/// 力度柱状图模式：bar 宽度取 note 长度（从 VelocityPoint 直接读取）。
pub(crate) fn push_velocity_bar_instances(
    instances: &mut Vec<CcBarInstance>,
    ctx: &VelocityBarContext<'_>,
) {
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
pub(crate) struct ValueBarContext<I: IntoIterator<Item = (f32, f32)>> {
    /// 待绘制的 (tick, 归一化值) 序列
    pub(crate) points: I,
    /// 面板左上角 X 坐标
    pub(crate) panel_x: f32,
    /// 面板实际 Y 坐标
    pub(crate) actual_panel_y: f32,
    /// 键盘区域宽度
    pub(crate) keyboard_width: f32,
    /// X 轴缩放
    pub(crate) zoom_x: f32,
    /// X 轴滚动偏移
    pub(crate) scroll_x: f32,
    /// 画布宽度
    pub(crate) canvas_size_x: f32,
    /// 绘图区域底部 Y 偏移
    pub(crate) max_y: f32,
    /// 曲线可用高度
    pub(crate) graph_height: f32,
    /// 柱条主色
    pub(crate) bar_color: [f32; 4],
}

/// 通用数值柱状条（Bend / CC 降级路径）：对给定 (tick, 归一化值) 序列绘制柱条。
pub(crate) fn push_value_bars<I: IntoIterator<Item = (f32, f32)>>(
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
