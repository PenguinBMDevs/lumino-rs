//! 画刷笔画渲染层测试：按钮定位 / 视口裁剪 / 抽稀 / 端帽分层几何

use super::*;
use crate::tests::test_helpers::seed_notes;
use lumino_core::Tool;

/// 构造画刷编辑器（2 轨，当前轨 1，画布 800x600）
fn brush_editor() -> Editor {
    let mut editor = Editor::new();
    editor.editor_state.tool = Tool::Brush;
    seed_notes(&mut editor, 2, 1, &[]);
    editor.editor_state.canvas.size_x = 800.0;
    editor.editor_state.canvas.size_y = 600.0;
    editor
}

/// 种一笔待确认笔画（逻辑坐标折线）
fn seed_stroke(editor: &mut Editor, points: &[(f32, f32)]) {
    let Some((first, rest)) = points.split_first() else {
        return;
    };
    let base = editor.editor_state.data.current_track;
    editor.editor_state.brush_tool.begin_stroke(*first, base);
    editor.editor_state.brush_tool.push_path_history();
    for point in rest {
        editor.editor_state.brush_tool.push_point(*point);
    }
    editor.editor_state.brush_tool.finish_stroke();
    editor.editor_state.brush_tool.update_top_path_history();
}

/// 默认视图下位于画布内的 key（画布 600 高、zoom_y 20 → 顶部约 30 个 key 可见）
const VISIBLE_KEY: f32 = 100.0;

#[test]
fn test_button_rects_inside_content_area() {
    let mut editor = brush_editor();
    seed_stroke(&mut editor, &[(0.0, 60.0), (1920.0, 62.0)]);
    let btns = brush_button_rects(&editor).expect("有笔画应有一对按钮");
    let content = content_bounds(&editor);
    for rect in [btns.confirm, btns.cancel] {
        assert!(rect.x >= content.x, "按钮不得超出内容区左缘");
        assert!(rect.y >= content.y, "按钮不得超出内容区顶缘");
        assert!(rect.x + rect.width <= content.x + content.width);
        assert!(rect.y + rect.height <= content.y + content.height);
    }
    // 只有一对按钮（√ 在左、× 在右）
    assert!(btns.cancel.x > btns.confirm.x);
}

#[test]
fn test_button_rects_none_without_strokes_or_wrong_tool() {
    let mut editor = brush_editor();
    assert!(brush_button_rects(&editor).is_none(), "无笔画不显示");

    seed_stroke(&mut editor, &[(0.0, 60.0)]);
    assert!(brush_button_rects(&editor).is_some());

    editor.editor_state.tool = Tool::Pencil;
    assert!(brush_button_rects(&editor).is_none(), "非画刷不显示");
}

#[test]
fn test_preview_points_decimate_but_keep_endpoints() {
    let mut editor = brush_editor();
    editor.editor_state.view.zoom_y = 20.0;
    // 逻辑上 200 个密集采样点（模拟高速拖动/高采样率鼠标），key 落在视口内
    let points: Vec<(f32, f32)> = (0..200)
        .map(|i| (i as f32 * 0.5, VISIBLE_KEY + (i % 3) as f32 * 0.1))
        .collect();
    let bounds = Rectangle::new(Point::new(0.0, 0.0), Size::new(800.0, 600.0));
    let preview = preview_points(&editor, &points, bounds, 20.0);

    assert!(!preview.is_empty());
    assert!(
        preview.len() < points.len(),
        "亚像素位移必须被抽稀（{} → {}）",
        points.len(),
        preview.len()
    );
    let first = editor.line_pos_screen_pos(points[0]);
    let last = editor.line_pos_screen_pos(points[points.len() - 1]);
    assert!(
        (preview[0].x - first.x).abs() < 0.01,
        "首点必须保留（端帽位置）"
    );
    assert!(
        (preview[preview.len() - 1].x - last.x).abs() < 0.01,
        "末点必须保留（端帽位置）"
    );
}

#[test]
fn test_preview_points_culls_strokes_outside_viewport() {
    let editor = brush_editor();
    let bounds = Rectangle::new(Point::new(0.0, 0.0), Size::new(800.0, 600.0));
    // 极远处的笔画：整笔在视口外 → 不参与绘制（长笔画平移后不拖慢帧）
    let far = vec![(5_000_000.0, VISIBLE_KEY), (5_000_100.0, VISIBLE_KEY + 2.0)];
    assert!(preview_points(&editor, &far, bounds, 20.0).is_empty());
}

#[test]
fn test_preview_points_cap_for_very_long_stroke() {
    let mut editor = brush_editor();
    editor.editor_state.view.zoom_y = 20.0;
    // 视口内 4000 个互不重合的采样点 → 硬上限内
    let points: Vec<(f32, f32)> = (0..4000).map(|i| (i as f32 * 2.0, VISIBLE_KEY)).collect();
    let bounds = Rectangle::new(Point::new(0.0, 0.0), Size::new(20000.0, 600.0));
    let preview = preview_points(&editor, &points, bounds, 20.0);
    assert!(
        !preview.is_empty(),
        "可见长笔画必须有预览点（bounds={bounds:?}）"
    );
    assert!(
        preview.len() <= MAX_PREVIEW_POINTS,
        "预览点数必须有硬上界：{}",
        preview.len()
    );
}

#[test]
fn test_uniform_sample_keeps_first_and_last() {
    let points: Vec<Point> = (0..100).map(|i| Point::new(i as f32, 0.0)).collect();
    let sampled = uniform_sample(&points, 10);
    assert_eq!(sampled.len(), 10);
    assert_eq!(sampled[0], points[0]);
    assert_eq!(sampled[9], points[99]);
}

#[test]
fn test_cap_segment_angles_cover_layer_band() {
    // 端帽圆心在笔画总宽中点：圆心上方 = key 更高（屏幕 Y 更小）、下方 = 更低
    let center = Point::new(100.0, 200.0);
    let radius = 30.0;
    let unit = |v: f32| (v / radius).clamp(-1.0, 1.0);

    // 圆心处（y = center.y）→ θ = 0
    assert!(unit(center.y - center.y).asin().abs() < 1e-6);
    // 圆顶（y = center.y + radius，对应笔画最高 key 的边界）→ θ = π/2
    let theta_hi_top = unit(center.y + radius - center.y).asin();
    assert!((theta_hi_top - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
    // 圆底（y = center.y - radius，对应落笔 key 边界）→ θ = -π/2
    let theta_lo_bottom = unit(center.y - radius - center.y).asin();
    assert!((theta_lo_bottom + std::f32::consts::FRAC_PI_2).abs() < 1e-6);
    // 整帽（单层、粗细度 1）= 完整圆
    let _full = cap_segment_path(center, radius, center.y - radius, center.y + radius);
    // 最底层色块的角度区间必须落在 [-π/2, π/2] 内
    let theta_lo = unit(center.y - radius - center.y).asin();
    let theta_hi = unit(center.y - radius + 2.0 - center.y).asin();
    assert!(theta_lo >= -std::f32::consts::FRAC_PI_2 - 1e-6);
    assert!(theta_hi > theta_lo);
}
