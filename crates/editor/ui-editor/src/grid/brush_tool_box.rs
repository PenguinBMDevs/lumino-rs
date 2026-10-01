//! 画刷矢量笔画渲染：**覆盖格方块**（方形、所见即生成）+ 共享 √× 悬浮按钮
//!
//! 视觉规格（BRUSH-001 补充需求：不要圆头圆尾，改方形，所见即生成）：
//! - 预览 = 逐**覆盖格**画方块：`x ∈ [格起 tick, 格起 + 吸附精度)`、`y ∈ [key, key+1)`（屏幕像素），
//!   **与 √ 实际生成的音符逐格一致**（同源函数 `brush_pending_notes`），因此不存在
//!   "预览一套、生成一套"的几何偏差；
//! - 每层（key）颜色 = 该层音轨音符显示色加深 40%（含洋葱皮轨），key 边界硬切换；
//! - 同一 `(音轨, key)` 行内连续格合并为一个矩形（`brush_preview_runs`），
//!   方块数远小于音符数，且**只增不改**——追加采样点只会新增格子，不会移动已有格子
//!   （替代旧折线渲染：超过点数上限后等距重采样会整笔漂移 → 抖动，见 §RCA）。
//!
//! 性能：
//! - 离屏笔画/方块在绘制前按屏幕矩形剔除，绘制成本只与可见方块数相关；
//! - 几何计算与屏幕点抽稀无关（不再需要折线抽稀），块成本 O(覆盖格数 + 行段数)。
//!
//! 渲染路径选型：canvas 叠加层（与 `line_tool_box` 同范式），方块 = 轴对齐矩形填充，
//! 无需 GFX 管线/跨线程同步。

use crate::Editor;
use crate::grid::confirm_buttons::{BUTTON_SIZE, CANCEL_ICON, CONFIRM_ICON, draw_button};
use crate::grid::utils::content_bounds;
use iced_core::{Point, Rectangle, Size};
use iced_widget::canvas::{self, Geometry, Path};
use lumino_message::Tool;
use lumino_ui_core::Renderer;

/// 按钮组与笔画包围盒的间距
const BUTTON_SPACING: f32 = 8.0;

/// 悬浮按钮矩形（画布坐标）
#[derive(Debug, Clone, Copy)]
pub struct BrushButtonRects {
    /// √ 确认按钮
    pub confirm: Rectangle,
    /// × 取消按钮
    pub cancel: Rectangle,
}

/// 计算待确认笔画右侧共享悬浮按钮位置（垂直居中于笔画包围盒中心）
///
/// 注意：**不**按 `current_track == 0` 隐藏按钮（与曲线工具不同）——
/// 笔画的目标音轨由**层分配**决定（含落笔时记录的基准轨），与当前轨无关；
/// 若照抄曲线工具的 Conductor 隐藏规则，用户在待确认期间切到 Conductor 轨
/// 会导致笔画被卡死（既不能 √ 也不能 ×）。Conductor 只在**落笔**时拦截（`pressed.rs`）。
pub fn brush_button_rects(editor: &Editor) -> Option<BrushButtonRects> {
    if editor.current_tool() != Tool::Brush {
        return None;
    }
    if !editor.editor_state.brush_tool.has_pending() {
        return None;
    }
    let (min_x, max_x, min_y, max_y) = editor.brush_strokes_screen_bounds()?;
    let content = content_bounds(editor);
    if content.height < BUTTON_SIZE {
        return None;
    }
    let mid_x = (min_x + max_x) * 0.5;
    let mid_y = (min_y + max_y) * 0.5;

    let group_w = BUTTON_SIZE * 2.0 + BUTTON_SPACING;
    // 垂直中心钳制到内容区内，避免笔画 Y 向越界时按钮悬浮到键盘/标尺上方
    let center_y = mid_y.clamp(
        content.y + BUTTON_SIZE * 0.5,
        content.y + content.height - BUTTON_SIZE * 0.5,
    );
    // 水平位置：优先包围盒右侧，超出内容区右边缘时钳制到右边缘
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
    Some(BrushButtonRects { confirm, cancel })
}

/// 覆盖格行段的屏幕矩形：`[t_start, t_end)` 格 × `key` 一个 key 高
///
/// 横向：X = tick 格宽（吸附精度 × zoom_x），Y = 单 key 高（zoom_y）；
/// 纵向（转置）：X = 单 key 高，Y = tick 格宽。
///
/// `pub`：供渲染层测试与性能基准复用。
pub fn brush_cell_rect(editor: &Editor, t_start: i64, t_end: i64, key: u16) -> Rectangle {
    let view = &editor.editor_state.view;
    let snap = view.snap_precision.max(1.0);
    let key_h = view.zoom_y.max(1.0);
    let a = editor.line_pos_screen_pos((t_start as f32 * snap, key as f32));
    let b = editor.line_pos_screen_pos((t_end as f32 * snap, key as f32));
    if editor.editor_state.is_vertical_roll {
        // 纵向：tick 沿 Y（tick 越大越靠上），key 沿 X
        let y0 = a.y.min(b.y);
        let h = (a.y - b.y).abs().max(1.0);
        Rectangle::new(Point::new(a.x, y0), Size::new(key_h, h))
    } else {
        let x0 = a.x.min(b.x);
        let w = (b.x - a.x).abs().max(1.0);
        Rectangle::new(Point::new(x0, a.y), Size::new(w, key_h))
    }
}

/// 绘制全部待确认笔画（覆盖格方块）+ 共享 √× 悬浮按钮
///
/// 仅在画刷工具激活时绘制。
pub fn draw(
    editor: &Editor,
    renderer: &Renderer,
    _theme: &lumino_ui_core::Theme,
    bounds: Rectangle,
) -> Option<Geometry<Renderer>> {
    if editor.current_tool() != Tool::Brush {
        return None;
    }
    if !editor.editor_state.brush_tool.has_pending() {
        return None;
    }

    let mut frame = canvas::Frame::new(renderer, bounds.size());
    let mut has_content = false;
    // 逻辑空间可见 tick 区间：先按格剔除，再算屏幕矩形——
    // 长笔画（或滚出视口的部分）完全不构建矩形，绘制成本只与可见格相关。
    let view = &editor.editor_state.view;
    let snap = view.snap_precision.max(1.0);
    let corner_a = Point::new(bounds.x, bounds.y);
    let corner_b = Point::new(bounds.x + bounds.width, bounds.y + bounds.height);
    let (t_a, t_b) = (editor.pos_to_tick(corner_a), editor.pos_to_tick(corner_b));
    let (t_lo, t_hi) = (t_a.min(t_b), t_a.max(t_b));
    let (cell_lo, cell_hi) = (
        (t_lo / snap).floor() as i64 - 1,
        (t_hi / snap).ceil() as i64 + 1,
    );
    // key 方向同理（横向时可见 key 由 y 决定，纵向时由 x 决定）
    let (k_a, k_b) = (editor.pos_to_key(corner_a), editor.pos_to_key(corner_b));
    let (key_lo, key_hi) = (
        k_a.min(k_b).saturating_sub(1),
        k_a.max(k_b).saturating_add(1),
    );

    for (track, key, t_start, t_end) in editor.brush_preview_runs() {
        if t_end < cell_lo || t_start > cell_hi || !(key_lo..=key_hi).contains(&key) {
            continue; // 视口外（网格区间剔除，含 1 格余量）
        }
        // 行段 → 屏幕矩形（闭区间格 → 半开矩形）
        let rect = brush_cell_rect(editor, t_start, t_end + 1, key);
        // 屏幕矩形二次剔除（边界余量内的极端情形）
        if !rect.intersects(&bounds) {
            continue;
        }
        let color = editor.brush_track_color(track);
        let path = Path::rectangle(rect.position(), rect.size());
        frame.fill(&path, color);
        has_content = true;
    }

    if let Some(btns) = brush_button_rects(editor) {
        draw_button(
            &mut frame,
            btns.confirm,
            &CONFIRM_ICON,
            iced_core::Color::from_rgb8(46, 125, 50),
        );
        draw_button(
            &mut frame,
            btns.cancel,
            &CANCEL_ICON,
            iced_core::Color::from_rgb8(198, 40, 40),
        );
        has_content = true;
    }

    if has_content {
        Some(frame.into_geometry())
    } else {
        None
    }
}

#[cfg(test)]
mod tests;
