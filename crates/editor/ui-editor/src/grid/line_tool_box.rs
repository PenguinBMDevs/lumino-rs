//! 曲线工具贝塞尔路径渲染：多路径锚点 + 曲线段 + 控制柄 + 实心填充 + 共享 √× 按钮
//!
//! 支持多条路径同时存在（批量绘制）：
//! - 锚点：实心圆（醒目标注，端点可拖动、中间锚点自由移动）；
//! - 曲线段：**引导线**（固定 [`OUTLINE_GUIDE_WIDTH`] 像素，与粗细无关）+ **描边产物
//!   预览**——后者直接画 √ 会生成的音符块（[`Editor::path_outline_blocks`]，与写入走
//!   同一个 `stroke_band::outline_blocks`），所以"画出来多粗"就是"√ 后生成多高的音符带"。
//!   此前这里用 `Stroke::width = 粗细 × 单 key 高` 的**居中**描边，而产物是**单侧向外**
//!   铺 `N-1` 层——差值在竖直方向上最大到 2 倍（用户报的"宽度显示 X2"）；
//! - 控制柄：方块 + 锚点间辅助线（首锚点 out、尾锚点 in、中间 in+out），
//!   拖动控制柄弯曲曲线；
//! - 实心填充：颜料桶点击封闭区域后，内部格点以半透明色块显示
//!   （√ 确认时合并生成实心音符）；
//! - 按钮：全部路径包围盒右侧的一组 √（批量确认生成音符）/ ×（批量取消）
//!   悬浮按钮，与 i2m 区域框按钮共用同一套视觉（`confirm_buttons` 模块）。

use crate::Editor;
use crate::grid::confirm_buttons::BUTTON_SIZE;
use crate::grid::utils::content_bounds;
use lumino_editor_state::AnchorBend;
use lumino_message::Tool;
use lumino_ui_core::geom::{Point, Rectangle, Size};

/// 锚点半径（像素）
/// 锚点描边宽度（像素）
/// 控制柄方块边长（像素）
/// 控制柄辅助线宽度（像素）
/// 按钮组与路径包围盒的间距
const BUTTON_SPACING: f32 = 8.0;
/// 悬浮按钮矩形（画布坐标）
#[derive(Debug, Clone, Copy)]
pub struct LineButtonRects {
    /// √ 确认按钮
    pub confirm: Rectangle,
    /// × 取消按钮
    pub cancel: Rectangle,
}

/// 选中锚点的弯折切换按钮（DRAW-042）
#[derive(Debug, Clone, Copy)]
pub struct LineBendRects {
    /// 曲线弯折
    pub smooth: Rectangle,
    /// 折线弯折
    pub corner: Rectangle,
    /// 当前状态（用于高亮）
    pub current: AnchorBend,
}

/// 弯折切换按钮尺寸 / 间距 / 与锚点的垂直距离（画布坐标）
const BEND_BUTTON_W: f32 = 52.0;
const BEND_BUTTON_H: f32 = 20.0;
const BEND_BUTTON_GAP: f32 = 4.0;
const BEND_BUTTON_OFFSET: f32 = 14.0;

/// 当前选中锚点可切换弯折时，返回两个切换按钮的矩形
pub fn line_bend_button_rects(editor: &Editor) -> Option<LineBendRects> {
    if editor.current_tool() != Tool::Curve {
        return None;
    }
    let (path_idx, anchor_idx) = editor.editor_state.line_tool.selected_anchor()?;
    let path = editor.editor_state.line_tool.paths.get(path_idx)?;
    if anchor_idx == 0 || anchor_idx + 1 >= path.len() {
        return None;
    }
    let anchor = path.get(anchor_idx)?;
    let ap = editor.line_pos_screen_pos(anchor.pos);
    let content = content_bounds(editor);
    let total = BEND_BUTTON_W * 2.0 + BEND_BUTTON_GAP;
    if content.width < total + 4.0 || content.height < BEND_BUTTON_H + 4.0 {
        return None;
    }
    let x = (ap.x - total * 0.5).clamp(
        content.x + 2.0,
        (content.x + content.width - total - 2.0).max(content.x + 2.0),
    );
    let mut y = ap.y - BEND_BUTTON_H - BEND_BUTTON_OFFSET;
    if y < content.y + 2.0 {
        y = ap.y + BEND_BUTTON_OFFSET;
    }
    y = y.clamp(
        content.y + 2.0,
        (content.y + content.height - BEND_BUTTON_H - 2.0).max(content.y + 2.0),
    );
    Some(LineBendRects {
        smooth: Rectangle::new(Point::new(x, y), Size::new(BEND_BUTTON_W, BEND_BUTTON_H)),
        corner: Rectangle::new(
            Point::new(x + BEND_BUTTON_W + BEND_BUTTON_GAP, y),
            Size::new(BEND_BUTTON_W, BEND_BUTTON_H),
        ),
        current: anchor.bend,
    })
}

/// 全部路径锚点的屏幕包围盒 (min_x, max_x, min_y, max_y)
fn paths_bounds(editor: &Editor) -> Option<(f32, f32, f32, f32)> {
    let paths = &editor.editor_state.line_tool.paths;
    let mut bounds: Option<(f32, f32, f32, f32)> = None;
    for path in paths {
        for anchor in path {
            let p = editor.line_pos_screen_pos(anchor.pos);
            bounds = Some(match bounds {
                None => (p.x, p.x, p.y, p.y),
                Some((min_x, max_x, min_y, max_y)) => (
                    min_x.min(p.x),
                    max_x.max(p.x),
                    min_y.min(p.y),
                    max_y.max(p.y),
                ),
            });
        }
    }
    bounds
}

/// 计算全部路径右侧共享悬浮按钮位置（垂直居中于包围盒中心）
///
/// 按钮组钳制到卷帘内容区内：路径移出/越界时按钮仍保持完整可见可点。
pub fn line_button_rects(editor: &Editor) -> Option<LineButtonRects> {
    // Conductor 音轨（track 0）：整工具不可用，不计算 √× 按钮命中区
    if editor.editor_state.data.current_track == 0 {
        return None;
    }
    if editor.current_tool() != Tool::Curve {
        return None;
    }
    // 至少存在一条完整路径才显示按钮
    if !editor.editor_state.line_tool.is_complete() {
        return None;
    }
    let (min_x, max_x, min_y, max_y) = paths_bounds(editor)?;
    let content = content_bounds(editor);
    // 内容区高度不足以容纳单个按钮时（异常布局）不显示按钮
    if content.height < BUTTON_SIZE {
        return None;
    }
    let mid_x = (min_x + max_x) * 0.5;
    let mid_y = (min_y + max_y) * 0.5;

    let group_w = BUTTON_SIZE * 2.0 + BUTTON_SPACING;
    // 垂直中心钳制到内容区内，避免路径 Y 向越界时按钮悬浮到键盘/标尺上方
    let center_y = mid_y.clamp(
        content.y + BUTTON_SIZE * 0.5,
        content.y + content.height - BUTTON_SIZE * 0.5,
    );
    // 水平位置：优先包围盒右侧，超出内容区右边缘时钳制到右边缘
    let x0 = (mid_x + BUTTON_SPACING).min(content.x + content.width - group_w - BUTTON_SPACING);
    // 内容区过窄无法容纳按钮组时（异常布局）不显示按钮
    if x0 < content.x + BUTTON_SPACING {
        return None;
    }
    let y0 = center_y - BUTTON_SIZE * 0.5;
    let confirm = Rectangle::new(Point::new(x0, y0), Size::new(BUTTON_SIZE, BUTTON_SIZE));
    let cancel = Rectangle::new(
        Point::new(x0 + BUTTON_SIZE + BUTTON_SPACING, y0),
        Size::new(BUTTON_SIZE, BUTTON_SIZE),
    );
    Some(LineButtonRects { confirm, cancel })
}

/// 绘制全部路径（锚点 + 贝塞尔段 + 控制柄）+ 共享 √× 悬浮按钮
///
/// 几何在**曲线工具或鼠标工具（图形选中）**激活时绘制（后者让「画完切过去」的图案不消失，
/// 见 `Editor::pending_preview_visible`）；√× 按钮仍只属于曲线工具。
#[cfg(test)]
mod tests;

/// 曲线填充预览（与 √ 生成的音符同源）。
#[derive(Debug, Clone, Default)]
pub struct FillPreview {
    /// 切分档位开启时的逐条音符块 (start_tick, end_tick, key)；空 = 走矢量闭环。
    pub blocks: Vec<(f32, f32, f32)>,
    /// 需要显示的内部填充闭环（tick/key，首尾重复）。
    pub loops: Vec<Vec<(f32, f32)>>,
}

/// 计算曲线填充预览（与 √ 生成的音符同源）。
pub fn fill_preview(editor: &Editor) -> Option<FillPreview> {
    let region = crate::interaction::line_tool::fill::region::fill_region(editor)?;
    Some(FillPreview {
        blocks: region.blocks,
        loops: region.filled_loops,
    })
}
