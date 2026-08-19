//! CC 柱状条实例构建（编辑器状态数据 → 实例列表）

use super::super::core::{CcBarColors, CcBarData, CcBarInstance, CcBarViewParams};
use super::TOOLBAR_HEIGHT;
use super::velocity::{
    ValueBarContext, VelocityBarContext, VelocityCurveContext, push_value_bars,
    push_velocity_bar_instances, push_velocity_curve_instances,
};
use crate::automation::{AUTOMATION_NODE_COLOR, AutomationViewParams, build_lane_instances};
use lumino_note_core::EditMode;

// ─── CC 面板布局常量（单一来源，各函数共用，禁止局部重定义） ────────
const PANEL_PADDING_Y: f32 = 12.0;
const RESIZE_HANDLE_HEIGHT: f32 = 5.0;
const H_SCROLLBAR_HEIGHT: f32 = 20.0;

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
            AUTOMATION_NODE_COLOR,
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
