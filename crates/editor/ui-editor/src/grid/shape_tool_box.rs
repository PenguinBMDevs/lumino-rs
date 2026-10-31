//! 形状工具渲染：待确认图形 + 实时拖拽预览 + 共享 √× 按钮
//!
//! 与 `line_tool_box` 同构：拖拽拉出外接框 → 矢量预览（矩形/圆/三角）→
//! 待确认图形悬停 √× 按钮（确认生成音符 / 取消清空）。
//!
//! 形状以矢量绘制（与网格精度无关），填充桶开启时内部以半透明色块显示，
//! √ 确认后成为实心音符（见 `interaction::shape_tool::confirm_shape_tool`）。
//!
//! **描边预览与产物同源**（用户报"宽度显示 X2"的修复）：`filled = false` 时预览画的
//! 就是 √ 会生成的**音符块**（[`Editor::shape_outline_blocks`]，与写入走同一个
//! `stroke_band::outline_blocks`）；矢量轮廓降级为固定宽度的**引导线**
//! （[`OUTLINE_GUIDE_WIDTH`]，与粗细无关）。此前预览用 `Stroke::width = 粗细 × 单 key 高`
//! 的居中描边——产物是**单侧向外**铺 `N-1` 层，竖直边在粗细 2 时预览比产物宽一倍。

use crate::Editor;
use crate::grid::confirm_buttons::{BUTTON_SIZE, CANCEL_ICON, CONFIRM_ICON, draw_button};
use crate::grid::utils::{OUTLINE_GUIDE_WIDTH, content_bounds, fill_note_blocks};
use crate::interaction::shape_bridge::polylines_of_model;
use iced_core::{Color, Point, Rectangle, Size};
use iced_widget::canvas::{self, Geometry, Path, Stroke};
use lumino_editor_state::shape_tool::{ShapeSpec, effective_rect, shape_vertices};
use lumino_editor_state::{ComposedKind, PendingComposed, ShapeKind, ShapeNote};
use lumino_ui_core::Renderer;

/// 填充色块透明度（半透明叠加在网格上，√ 确认后成为实心音符）
const FILL_ALPHA: f32 = 0.35;
/// 按钮组与图形包围盒的间距
const BUTTON_SPACING: f32 = 8.0;

/// 悬浮按钮矩形（画布坐标）
#[derive(Debug, Clone, Copy)]
pub struct ShapeButtonRects {
    /// √ 确认按钮
    pub confirm: Rectangle,
    /// × 取消按钮
    pub cancel: Rectangle,
}

/// 单个图形在屏幕坐标下的包围盒 (min_x, max_x, min_y, max_y)
fn shape_screen_aabb(editor: &Editor, spec: ShapeSpec) -> (f32, f32, f32, f32) {
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut acc = |p: Point| {
        min_x = min_x.min(p.x);
        max_x = max_x.max(p.x);
        min_y = min_y.min(p.y);
        max_y = max_y.max(p.y);
    };
    // 统一应用屏幕空间 Shift 正图形约束，使包围盒与生成音符一致
    let px_per_tick = editor.editor_state.view.zoom_x;
    let px_per_key = editor.editor_state.view.zoom_y;
    let rect = effective_rect(spec, px_per_tick, px_per_key);
    match spec.kind {
        ShapeKind::Circle => {
            let (cx0, cy0, cx1, cy1) = rect;
            let mx = (cx0 + cx1) / 2.0;
            let my = (cy0 + cy1) / 2.0;
            acc(editor.line_pos_screen_pos((cx0, my)));
            acc(editor.line_pos_screen_pos((cx1, my)));
            acc(editor.line_pos_screen_pos((mx, cy0)));
            acc(editor.line_pos_screen_pos((mx, cy1)));
        }
        _ => {
            let verts = shape_vertices(spec, px_per_tick, px_per_key).unwrap_or_default();
            for (t, k) in verts {
                acc(editor.line_pos_screen_pos((t, k)));
            }
        }
    }
    (min_x, max_x, min_y, max_y)
}

/// 组合结果在文档坐标下的包围盒。
///
/// 027 的副本由 `ShapeOp::CopyAlong` 生成，不在 `model.geometry` 里；
/// 悬浮按钮/预览需要用组合后的音符范围，否则会框不住副本。
fn composed_bounds(composed: &PendingComposed) -> Option<(f32, f32, f32, f32)> {
    if composed.kind == ComposedKind::CopyAlong && !composed.notes.is_empty() {
        let mut lo_t = f32::INFINITY;
        let mut hi_t = f32::NEG_INFINITY;
        let mut lo_k = f32::INFINITY;
        let mut hi_k = f32::NEG_INFINITY;
        for note in &composed.notes {
            lo_t = lo_t.min(note.tick);
            hi_t = hi_t.max(note.tick + note.length.max(0.0));
            lo_k = lo_k.min(f32::from(note.key));
            hi_k = hi_k.max(f32::from(note.key));
        }
        if lo_t.is_finite() && hi_t.is_finite() {
            return Some((lo_t, lo_k, hi_t, hi_k));
        }
    }
    composed.model.geometry.bounds()
}

/// 027/028 的效果不能只靠轮廓表达：直接画组合后的音符块。
///
/// - 027：副本落在路径上，音符块就是 ✓ 会提交的最终位置；
/// - 028：逐音符按力度调制透明度，渐强在提交前可见。
fn draw_composed_notes(
    frame: &mut canvas::Frame<Renderer>,
    editor: &Editor,
    composed: &PendingComposed,
    base_color: Color,
) -> bool {
    if composed.notes.is_empty() {
        return false;
    }
    match composed.kind {
        ComposedKind::CopyAlong => {
            let blocks: Vec<(f32, f32, u16)> = composed.notes.iter().map(note_block).collect();
            fill_note_blocks(
                frame,
                editor,
                &blocks,
                Color {
                    a: 0.55,
                    ..base_color
                },
            )
        }
        ComposedKind::Velocity => {
            let mut drawn = false;
            for note in &composed.notes {
                let alpha = (0.15 + f32::from(note.velocity) / 127.0 * 0.55).clamp(0.15, 0.7);
                drawn |= fill_note_blocks(
                    frame,
                    editor,
                    &[note_block(note)],
                    Color {
                        a: alpha,
                        ..base_color
                    },
                );
            }
            drawn
        }
        _ => false,
    }
}

/// 音符 → `fill_note_blocks` 的 `(tick_lo, tick_hi, key)`。
fn note_block(note: &ShapeNote) -> (f32, f32, u16) {
    (
        note.tick,
        (note.tick + note.length).max(note.tick + 1.0),
        note.key,
    )
}

/// 计算待确认图形 + 拖拽预览的共享悬浮按钮位置（垂直居中于包围盒中心）
pub fn shape_button_rects(editor: &Editor) -> Option<ShapeButtonRects> {
    // Conductor 音轨（track 0）：整工具不可用，不计算 √× 按钮命中区
    if editor.editor_state.data.current_track == 0 {
        return None;
    }
    if editor.current_tool() != lumino_message::Tool::Shape {
        return None;
    }
    let shape_tool = &editor.editor_state.shape_tool;
    if !shape_tool.has_pending() && editor.editor_state.pending_composed.is_empty() {
        return None;
    }

    let mut bounds: Option<(f32, f32, f32, f32)> = None;
    for shape in &shape_tool.shapes {
        let b = shape_screen_aabb(editor, shape.spec());
        bounds = Some(match bounds {
            None => b,
            Some((min_x, max_x, min_y, max_y)) => (
                min_x.min(b.0),
                max_x.max(b.1),
                min_y.min(b.2),
                max_y.max(b.3),
            ),
        });
    }
    for composed in &editor.editor_state.pending_composed {
        let Some((t0, k0, t1, k1)) = composed_bounds(composed) else {
            continue;
        };
        let a = editor.line_pos_screen_pos((t0, k0));
        let b = editor.line_pos_screen_pos((t1, k1));
        let (min_x, max_x) = (a.x.min(b.x), a.x.max(b.x));
        let (min_y, max_y) = (a.y.min(b.y), a.y.max(b.y));
        bounds = Some(match bounds {
            None => (min_x, max_x, min_y, max_y),
            Some((x0, x1, y0, y1)) => (x0.min(min_x), x1.max(max_x), y0.min(min_y), y1.max(max_y)),
        });
    }

    let (min_x, max_x, min_y, max_y) = bounds?;
    let content = content_bounds(editor);
    if content.height < BUTTON_SIZE {
        return None;
    }
    let mid_x = (min_x + max_x) * 0.5;
    let mid_y = (min_y + max_y) * 0.5;

    let group_w = BUTTON_SIZE * 2.0 + BUTTON_SPACING;
    let center_y = mid_y.clamp(
        content.y + BUTTON_SIZE * 0.5,
        content.y + content.height - BUTTON_SIZE * 0.5,
    );
    let x0 = (mid_x + BUTTON_SPACING).min(content.x + content.width - group_w - BUTTON_SPACING);
    if x0 < content.x + BUTTON_SPACING {
        return None;
    }
    let y0 = center_y - BUTTON_SIZE * 0.5;
    let confirm = Rectangle::new(Point::new(x0, y0), Size::new(BUTTON_SIZE, BUTTON_SIZE));
    let cancel = Rectangle::new(
        Point::new(x0 + BUTTON_SIZE + BUTTON_SPACING, y0),
        Size::new(BUTTON_SIZE, BUTTON_SIZE),
    );
    Some(ShapeButtonRects { confirm, cancel })
}

/// 生成椭圆路径（本 iced 版本无 `Path::ellipse`，改用多边形逼近）
fn ellipse_path(center: Point, rx: f32, ry: f32) -> Path {
    const SEGMENTS: usize = 64;
    Path::new(|p| {
        for i in 0..=SEGMENTS {
            let a = (i as f32 / SEGMENTS as f32) * 2.0 * std::f32::consts::PI;
            let x = center.x + rx * a.cos();
            let y = center.y + ry * a.sin();
            if i == 0 {
                p.move_to(Point::new(x, y));
            } else {
                p.line_to(Point::new(x, y));
            }
        }
        p.close();
    })
}

/// 绘制单个矢量形状（引导轮廓 + 可选填充 + **描边产物预览**），在 `draw` 内联调用
/// （避免泛型渲染器约束）
///
/// - **引导轮廓**：固定 [`OUTLINE_GUIDE_WIDTH`]、与粗细无关——只回答"图形边界在哪"；
/// - **填充**：半透明色块（`filled` 时），与填充腿生成的格点同区域；
/// - **描边产物预览**：`filled == false` 时直接画**会生成的音符块**
///   （[`Editor::shape_outline_blocks`]，与 √ 写入同源）。此前这里是
///   `Stroke::width = 全局粗细 × 单 key 高` 的**居中**描边：竖直边在粗细 2 时比产物
///   宽一倍（产物单侧向外只铺 1 层），即用户报的"宽度显示 X2"缺陷。
#[inline]
fn draw_one_shape(
    frame: &mut canvas::Frame<Renderer>,
    editor: &Editor,
    spec: ShapeSpec,
    filled: bool,
    color: Color,
) {
    // 统一应用屏幕空间 Shift 正图形约束，使预览与生成音符一致
    let px_per_tick = editor.editor_state.view.zoom_x;
    let px_per_key = editor.editor_state.view.zoom_y;
    let rect = effective_rect(spec, px_per_tick, px_per_key);
    match spec.kind {
        ShapeKind::Circle => {
            let (cx0, cy0, cx1, cy1) = rect;
            let mx = (cx0 + cx1) / 2.0;
            let my = (cy0 + cy1) / 2.0;
            let center = editor.line_pos_screen_pos((mx, my));
            let left = editor.line_pos_screen_pos((cx0, my));
            let right = editor.line_pos_screen_pos((cx1, my));
            let top = editor.line_pos_screen_pos((mx, cy1));
            let bottom = editor.line_pos_screen_pos((mx, cy0));
            let rx = (right.x - left.x).abs() * 0.5;
            let ry = (bottom.y - top.y).abs() * 0.5;
            let path = ellipse_path(center, rx, ry);
            if filled {
                let fill = Color::from_rgba(color.r, color.g, color.b, FILL_ALPHA);
                frame.fill(&path, fill);
            }
            let stroke = Stroke::default()
                .with_width(OUTLINE_GUIDE_WIDTH)
                .with_color(color);
            frame.stroke(&path, stroke);
        }
        _ => {
            let verts = match shape_vertices(spec, px_per_tick, px_per_key) {
                Some(v) => v,
                None => return,
            };
            let points: Vec<Point> = verts
                .iter()
                .map(|&(t, k)| editor.line_pos_screen_pos((t, k)))
                .collect();
            let path = Path::new(|p| {
                if let Some(first) = points.first() {
                    p.move_to(*first);
                    for pt in points.iter().skip(1) {
                        p.line_to(*pt);
                    }
                    p.close();
                }
            });
            if filled {
                let fill = Color::from_rgba(color.r, color.g, color.b, FILL_ALPHA);
                frame.fill(&path, fill);
            }
            let stroke = Stroke::default()
                .with_width(OUTLINE_GUIDE_WIDTH)
                .with_color(color);
            frame.stroke(&path, stroke);
        }
    }
    // 描边产物预览 = √ 会生成的音符块（填充图形由权威源返回空，不画描边预览）
    fill_note_blocks(
        frame,
        editor,
        &editor.shape_outline_blocks(spec, filled),
        color,
    );
}

/// 绘制全部待确认图形 + 实时拖拽预览 + 共享 √× 悬浮按钮
///
/// 几何在**形状工具或鼠标工具（图形选中）**激活时绘制（后者让「画完切过去」的图案不消失，
/// 见 `Editor::pending_preview_visible`）；√× 按钮仍只属于形状工具。
pub fn draw(
    editor: &Editor,
    renderer: &Renderer,
    theme: &lumino_ui_core::Theme,
    bounds: Rectangle,
) -> Option<Geometry<Renderer>> {
    // Conductor 音轨（track 0）：整工具不可用，不绘制
    if editor.editor_state.data.current_track == 0 {
        return None;
    }
    if !editor.pending_preview_visible(lumino_message::Tool::Shape) {
        return None;
    }
    let shape_tool = &editor.editor_state.shape_tool;
    if !shape_tool.has_pending()
        && !shape_tool.is_dragging()
        && editor.editor_state.pending_composed.is_empty()
    {
        return None;
    }

    let mut frame = canvas::Frame::new(renderer, bounds.size());
    let mut has_content = false;
    let base_color = theme.extended_palette().primary.strong.color;

    // 待确认图形（虚线感由填充/轮廓区分，这里统一实线轮廓）
    for shape in &shape_tool.shapes {
        draw_one_shape(&mut frame, editor, shape.spec(), shape.filled, base_color);
        has_content = true;
    }

    // DRAW-041C：Bake 前组合结果的预览（仍是 pending，✓ 才提交）
    for composed in &editor.editor_state.pending_composed {
        for points in polylines_of_model(&composed.model) {
            let screen: Vec<Point> = points
                .iter()
                .map(|&(tick, key)| editor.line_pos_screen_pos((tick, key)))
                .collect();
            if screen.len() < 2 {
                continue;
            }
            let path = Path::new(|p| {
                p.move_to(screen[0]);
                for point in &screen[1..] {
                    p.line_to(*point);
                }
            });
            frame.stroke(
                &path,
                Stroke::default().with_color(base_color).with_width(2.0),
            );
            has_content = true;
        }
        // 027/028 的产物无法只靠轮廓表达：叠加音符块预览。
        has_content |= draw_composed_notes(&mut frame, editor, composed, base_color);
    }

    // 实时拖拽预览
    if let Some((spec, filled)) = shape_tool.preview_rect(editor.shift_pressed()) {
        draw_one_shape(&mut frame, editor, spec, filled, base_color);
        has_content = true;
    }

    // 共享悬浮按钮（存在待确认图形后显示）
    if let Some(btns) = shape_button_rects(editor) {
        draw_button(
            &mut frame,
            btns.confirm,
            &CONFIRM_ICON,
            Color::from_rgb8(46, 125, 50),
        );
        draw_button(
            &mut frame,
            btns.cancel,
            &CANCEL_ICON,
            Color::from_rgb8(198, 40, 40),
        );
        has_content = true;
    }

    if has_content {
        Some(frame.into_geometry())
    } else {
        None
    }
}
