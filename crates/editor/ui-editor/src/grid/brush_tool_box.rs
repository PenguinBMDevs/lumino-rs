//! 画刷矢量笔画渲染：**可见窗口计算 / 行段几何** + 共享 √× 悬浮按钮
//!
//! 视觉规格（BRUSH-001 补充需求：方形、所见即生成）：
//! - 方块 = 每个**覆盖格**一个矩形：`x ∈ [格起 tick, 格起 + 吸附精度)`、
//!   `y ∈ [key, key+1)`；**与 √ 实际生成的音符逐格一致**（同源
//!   `brush_pending_notes`），不存在"预览一套、生成一套"的几何偏差；
//! - 每层（key）颜色 = 该层音轨音符显示色（含洋葱皮轨），key 边界硬切换；
//! - 同一 `(音轨, key)` 行内连续格合并为一个矩形（`brush_preview_runs`），
//!   **只增不改**——追加采样点只会新增格子，不会移动已有格子
//!   （替代旧折线渲染：超过点数上限后等距重采样会整笔漂移 → 抖动，见 §RCA）。
//!
//! ⚠️ **渲染通路（§18）**：方块本体**不再由 canvas 绘制**，改由
//! [`Editor::brush_preview_note_instances`] 产出实例、走音符的 wgpu 预览通路
//! （与 √ 生成的音符同一个着色器）。原因：`Frame::fill` 每次调用都要跑一遍 lyon
//! 细分 + 按色查 buffer，实测 8 万方块 = 单帧 **81.6ms**；而块数 = 可见格 × 粗细度，
//! 缩小时铺满一屏，CPU 细分结构上守不住 60fps。
//! 本模块保留：可见窗口计算（`brush_visible_window`，同时供窗口化栅格化与实例筛选）、
//! 行段→屏幕矩形剔除（`brush_run_screen_rect`，实例筛选复用同一条口径）、
//! 以及 √× 悬浮按钮的 canvas 绘制（微秒级）。
//!
//! ⚠️ 坐标系契约（裁剪错位 BUG 的根因）：`Program::draw` 的 `bounds` 是**窗口坐标**
//! （含左侧栏/工具栏偏移），帧内绘制坐标却是**画布局部坐标**——可见区域只取
//! `bounds.size()`，处理见 [`brush_visible_window`]；详见 §16 RCA。
//!
//! 渲染路径选型：canvas 叠加层（与 `line_tool_box` 同范式），方块 = 轴对齐矩形填充，
//! 无需 GFX 管线/跨线程同步。

use crate::Editor;
use crate::grid::confirm_buttons::{BUTTON_SIZE, CANCEL_ICON, CONFIRM_ICON, draw_button};
use crate::grid::utils::{clip_rect, content_bounds};
use iced_core::{Point, Rectangle, Size};
use iced_widget::canvas::{self, Geometry};
use lumino_editor_state::brush_tool::cov::{self, MAX_KEY};
use lumino_message::Tool;
use lumino_ui_core::Renderer;

/// 按钮组与笔画包围盒的间距
const BUTTON_SPACING: f32 = 8.0;

/// 由 iced 传入的 `bounds` 计算**画布局部坐标**下的可见窗口（逻辑区间）
///
/// # 坐标系契约（★ 画刷裁剪错位 BUG 的根因，见 §16 RCA）
///
/// `Program::draw` 收到的 `bounds` 是**父/窗口坐标系**矩形——`position` 是该画布
/// 组件在窗口里的偏移（本项目实际值 = 左侧栏宽 + 音轨列表 160、工具栏高 + 标题栏
/// 30，见 `host/render/viewport.rs`），而**帧内所有绘制坐标都是画布局部坐标**
/// （原点 = 画布左上角）：iced 在调用 `Program::draw` 前先
/// `renderer.with_translation(Vector::new(bounds.x, bounds.y), ..)`
/// （`iced_widget/src/canvas.rs`），再把 `bounds`（原样、未归一化）交给 program。
///
/// 因此可见区域只能是 `size()`，**绝不能把 `bounds` 当局部矩形用**：
/// 旧实现用 `Point::new(bounds.x, bounds.y)` 当可见窗口左上角，等于把窗口整体
/// 右移 `offset_x`、下移 `offset_y`，于是笔画被裁掉左上角一块——
/// 顶部约 `(offset_y - ruler_height) / zoom_y` 个 key、左侧约 `offset_x - keyboard_width`
/// 像素宽的格子（用户实测：顶部 3 个 KEY + 左侧部分）。
///
/// 本函数只读 `bounds.size()`；`position` 被显式忽略是**有意为之**，
/// 回归测试 `test_visible_window_is_independent_of_widget_offset` 锁死该语义。
///
/// 返回值即 [`cov::CellWindow`]：既做"整段视口外"剔除，也是**窗口化栅格化**的范围
/// （预览只对窗口内的格做覆盖计算，见 `brush_preview_runs_in_window`）。
pub fn brush_visible_window(editor: &Editor, bounds: Rectangle) -> cov::CellWindow {
    let snap = editor.editor_state.view.snap_precision.max(1.0);
    // 局部坐标四角：原点 → 画布尺寸（忽略 bounds.position）
    let corner_a = Point::new(0.0, 0.0);
    let corner_b = Point::new(bounds.width, bounds.height);
    let (t_a, t_b) = (editor.pos_to_tick(corner_a), editor.pos_to_tick(corner_b));
    let (t_lo, t_hi) = (t_a.min(t_b), t_a.max(t_b));
    let (k_a, k_b) = (editor.pos_to_key(corner_a), editor.pos_to_key(corner_b));
    cov::CellWindow {
        // 1 格余量：边界上的半个格（吸精度 × zoom_x 跨像素）不外泄
        cell_lo: (t_lo / snap).floor() as i64 - 1,
        cell_hi: (t_hi / snap).ceil() as i64 + 1,
        key_lo: k_a.min(k_b).saturating_sub(1),
        key_hi: k_a.max(k_b).saturating_add(1).min(MAX_KEY),
    }
}

/// 行段 `(音轨, key, 起格, 止格)` → 最终可绘制的屏幕矩形（不可见返回 `None`）
///
/// 三级剔除，顺序即成本顺序（先逻辑区间，再屏幕矩形，最后内容区裁剪）：
/// 1. 逻辑可见窗口剔除（整段在视口外的行段完全不构建矩形）；
/// 2. 屏幕矩形与**画布局部** bounds 求交（`canvas_bounds` 必须是
///    `Rectangle::new(Point::ORIGIN, bounds.size())`，见 [`brush_visible_window`]）；
/// 3. 裁剪到卷帘内容区 [`content_bounds`] —— 键盘列/标尺带由先绘制的键盘、标尺
///    覆盖层遮住音符，预览也必须被遮住才是"所见即生成后效果"；
///    否则笔画会把方块画到钢琴键和标尺上面。
///
/// `pub`：绘制与测试共用同一条过滤管线，避免"测试用一套、绘制用另一套"。
pub fn brush_run_screen_rect(
    editor: &Editor,
    window: &cov::CellWindow,
    canvas_bounds: Rectangle,
    run: (usize, u16, i64, i64),
) -> Option<Rectangle> {
    let (_track, key, t_start, t_end) = run;
    if t_end < window.cell_lo || t_start > window.cell_hi {
        return None;
    }
    if key < window.key_lo || key > window.key_hi {
        return None;
    }
    // 闭区间格 → 半开矩形
    let rect = brush_cell_rect(editor, t_start, t_end + 1, key);
    if !rect.intersects(&canvas_bounds) {
        return None;
    }
    clip_rect(rect, content_bounds(editor))
}

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

/// 绘制待确认笔画的 **√× 悬浮按钮**（方块本体已改走 wgpu 预览实例）
///
/// §18：预览方块不再由 canvas 绘制——`Frame::fill` 每次调用都要跑一遍 lyon 细分
/// 并按颜色查 buffer 表，实测 8 万方块 = 单帧 **81.6ms**（puffin：整帧只有它超标），
/// 而块数 = 可见格 × 粗细度，缩小时铺满一屏，CPU 细分结构上守不住 60fps。
/// 现在方块由 [`Editor::brush_preview_note_instances`] 产出、走音符的 wgpu 预览
/// 通路（与 √ 生成的音符同一个着色器）；本函数只负责按钮（2 次图片绘制，微秒级）。
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

    let btns = brush_button_rects(editor)?;
    let mut frame = canvas::Frame::new(renderer, bounds.size());
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
    Some(frame.into_geometry())
}

#[cfg(test)]
mod tests;
