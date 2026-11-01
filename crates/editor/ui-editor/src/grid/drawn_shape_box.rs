//! 已绘制图形叠加层：**常显轮廓**（鼠标工具）+ **选中高亮** + **选中选框** + **框选矩形**
//!
//! 「鼠标工具」（`Tool::ShapeSelect`）需要「看得见才点得中」：
//! - **常显轮廓**：鼠标工具激活时，把当前音轨上全部可见图形以弱色细线画出，
//!   使用户一眼看到「哪些图案可选中」。否则切换工具后画布上只剩音符，
//!   图形轮廓不可见，表现为「图案消失了、无从框选」。
//! - **选中高亮**：被选中的每一个图形都叠加发光描边（外发光 + 实线双描边），与
//!   `shape_tool_box` 的待确认预览区分（预览为细线，高亮更粗且带光晕）。
//! - **选中选框**：选中集（多选时为**并集**）的屏幕外接框 + 四角手柄，把「当前选中了哪些
//!   图形、从哪里可以拖动它们」明示出来。框内区域就是可拖动区（命中判定见
//!   `interaction::drawn_shape` 的 `point_in_selection_box`）——渲染与命中同源。
//! - **框选矩形**：空白处拉框时实时绘制半透明选择框。
//! - **编辑图形**：020 进入编辑态时隐藏选中选框，改画细编辑框 + 8 个箱体锚点
//!   （未 Bake 组合再加中心锚点），与 ShapeSelect 选框严格分层。
//!
//! 渲染口径与命中测试同源：形状工具图形走 `shape_vertices` / 椭圆，折线
//! （曲线展平路径 / 画刷笔画）直接连点成线。仅在图形属于**当前音轨**且未被
//! 撤销隐藏时绘制。

use crate::Editor;
use iced_core::{Color, Point, Rectangle, Size};
use iced_widget::canvas::{self, Geometry, Path, Stroke};
use lumino_ui_core::{Renderer, Theme};

pub(super) use super::drawn_shape_outline::outline_path;

/// 外发光描边宽度（像素）
const GLOW_WIDTH: f32 = 7.0;
/// 实线描边宽度（像素）
const CORE_WIDTH: f32 = 2.5;
/// 外发光透明度
const GLOW_ALPHA: f32 = 0.30;
/// 常显轮廓线宽（像素）
const IDLE_WIDTH: f32 = 1.5;
/// 常显轮廓透明度（弱化，避免盖过音符）
const IDLE_ALPHA: f32 = 0.55;
/// 框选矩形填充透明度
const MARQUEE_FILL_ALPHA: f32 = 0.12;
/// 框选矩形描边透明度
const MARQUEE_STROKE_ALPHA: f32 = 0.75;
/// 选中选框描边宽度（像素）
const BOX_STROKE_WIDTH: f32 = 1.5;
/// 选中选框四角手柄边长（像素）
const BOX_HANDLE_SIZE: f32 = 6.0;
/// 选中选框描边透明度
const BOX_STROKE_ALPHA: f32 = 0.85;
/// 编辑图形：编辑框描边透明度
const EDIT_BOX_ALPHA: f32 = 0.65;
/// 编辑图形：锚点边长（像素）
const EDIT_HANDLE_SIZE: f32 = 8.0;

/// 选中集内**全部**图形的轮廓（同轨；含整组拖拽实时预览偏移）
///
/// 多选时逐个选中图形都画发光描边；拖拽作用于整个选中集，故所有图形套用同一预览偏移。
/// **例外**：待确认镜像的几何已随拖动即时落位（见
/// `Editor::sync_pending_drag_geometry`），偏移必须取 0——否则描边会与选框 / owning
/// 预览分叉出双倍位移。
pub(super) fn selected_outlines(editor: &Editor) -> Vec<Path> {
    let track = editor.editor_state.data.current_track;
    let (dtick, dkey) = editor.drag_preview_delta();
    editor
        .editor_state
        .shape_select
        .selected_shapes_on(track)
        .filter_map(|s| {
            let (dt, dk) = if s.pending.is_some() {
                (0.0, 0.0)
            } else {
                (dtick, dkey)
            };
            outline_path(editor, s, dt, dk)
        })
        .collect()
}

/// 两点决定的轴对齐矩形路径（框选用）
fn rect_path(a: Point, b: Point) -> Path {
    let (x0, y0) = (a.x.min(b.x), a.y.min(b.y));
    let (x1, y1) = (a.x.max(b.x), a.y.max(b.y));
    Path::new(|p| {
        p.move_to(Point::new(x0, y0));
        p.line_to(Point::new(x1, y0));
        p.line_to(Point::new(x1, y1));
        p.line_to(Point::new(x0, y1));
        p.close();
    })
}

/// 叠加层绘制内容（与渲染后端解耦，便于单测「要不要画、画了什么」）
struct Overlay {
    /// 常显轮廓（鼠标工具下当前轨全部可见图形，排除已选中者）
    idle_paths: Vec<Path>,
    /// 选中高亮（逐个选中图形：发光 + 实线双描边）
    selected_paths: Vec<Path>,
    /// 选中集选框（并集外接框 + 四角手柄）
    selection_box: Option<Rectangle>,
    /// 框选拉框矩形
    marquee_path: Option<Path>,
    /// 020 编辑图形：编辑框（进入编辑态时**替代**选中选框）
    edit_box: Option<Rectangle>,
    /// 020 编辑图形：8 个箱体锚点 + 可选中心锚点（屏幕坐标）
    edit_handles: crate::interaction::shape_edit_ui::ShapeEditHandles,
}

impl Overlay {
    fn collect(editor: &Editor) -> Self {
        // 鼠标工具激活时把全部可见图形以弱色轮廓常显：没有它，「图案」在切换工具后
        // 就只剩音符、看不见可选中范围（用户反馈：切换后图案消失、无从框选）。
        // 其余工具只画选中高亮，避免常显轮廓干扰正常音符编辑。
        let select_mode = editor.current_tool() == lumino_message::Tool::ShapeSelect;
        let current_track = editor.editor_state.data.current_track;
        let idle_paths: Vec<Path> = if select_mode {
            editor
                .editor_state
                .shape_select
                .visible_on(current_track)
                .filter(|s| !editor.editor_state.shape_select.is_selected(s.id))
                .filter_map(|s| outline_path(editor, s, 0.0, 0.0))
                .collect()
        } else {
            Vec::new()
        };
        let marquee_path = editor.editor_state.shape_select.marquee().map(|m| {
            let (t0, t1, k0, k1) = m.rect();
            let a = editor.line_pos_screen_pos((t0, k0));
            let b = editor.line_pos_screen_pos((t1, k1));
            rect_path(a, b)
        });
        // 编辑图形态与 ShapeSelect 选框严格分离：进入后隐藏蓝色选框，
        // 只留 8+1 锚点与细编辑框，避免两层框叠在一起抢点击。
        let editing = editor.is_shape_edit_armed();
        Self {
            idle_paths,
            selected_paths: selected_outlines(editor),
            // 选中选框：与命中判定同源（`Editor::selection_box`），
            // 保证「看到的框」就是「抓得住的框」。
            selection_box: if editing {
                None
            } else {
                editor.selection_box()
            },
            marquee_path,
            edit_box: editor.shape_edit_box(),
            edit_handles: editor.shape_edit_handles(),
        }
    }

    /// 全部为空 → 不产生空几何
    fn is_empty(&self) -> bool {
        self.idle_paths.is_empty()
            && self.selected_paths.is_empty()
            && self.selection_box.is_none()
            && self.marquee_path.is_none()
            && self.edit_box.is_none()
            && self.edit_handles.box_points.is_empty()
            && self.edit_handles.center.is_none()
    }
}

/// 绘制已绘制图形叠加层（常显轮廓 / 选中高亮 / 选中选框 / 框选矩形）
///
/// 四者皆无时返回 `None`（不产生空几何）。
pub fn draw(
    editor: &Editor,
    renderer: &Renderer,
    theme: &Theme,
    bounds: Rectangle,
) -> Option<Geometry<Renderer>> {
    let palette = theme.extended_palette();
    let strong = palette.primary.strong.color;
    let idle = Color::from_rgba(
        palette.background.strongest.color.r,
        palette.background.strongest.color.g,
        palette.background.strongest.color.b,
        IDLE_ALPHA,
    );
    let marquee_color = palette.primary.base.color;

    let overlay = Overlay::collect(editor);
    if overlay.is_empty() {
        return None;
    }

    let mut frame = canvas::Frame::new(renderer, bounds.size());

    for path in &overlay.idle_paths {
        frame.stroke(
            path,
            Stroke::default().with_width(IDLE_WIDTH).with_color(idle),
        );
    }

    if !overlay.selected_paths.is_empty() {
        let glow = Color::from_rgba(strong.r, strong.g, strong.b, GLOW_ALPHA);
        for path in &overlay.selected_paths {
            frame.stroke(
                path,
                Stroke::default().with_width(GLOW_WIDTH).with_color(glow),
            );
            frame.stroke(
                path,
                Stroke::default().with_width(CORE_WIDTH).with_color(strong),
            );
        }
    }

    // 020 编辑图形：细编辑框 + 8 个方形锚点 + 中心锚点（未 Bake 组合才有）
    if let Some(rect) = overlay.edit_box {
        let box_color = Color::from_rgba(strong.r, strong.g, strong.b, EDIT_BOX_ALPHA);
        frame.stroke(
            &Path::rectangle(rect.position(), rect.size()),
            Stroke::default().with_width(1.0).with_color(box_color),
        );
    }
    let white = Color::WHITE;
    let half = EDIT_HANDLE_SIZE / 2.0;
    for point in &overlay.edit_handles.box_points {
        let square = Path::rectangle(
            Point::new(point.x - half, point.y - half),
            Size::new(EDIT_HANDLE_SIZE, EDIT_HANDLE_SIZE),
        );
        frame.fill(&square, white);
        frame.stroke(
            &square,
            Stroke::default().with_width(1.5).with_color(strong),
        );
    }
    if let Some(center) = overlay.edit_handles.center {
        let radius = half + 1.0;
        let circle = Path::circle(center, radius);
        frame.fill(&circle, white);
        frame.stroke(
            &circle,
            Stroke::default().with_width(1.5).with_color(strong),
        );
        // 十字：示意「整体平移」
        let arm = radius - 2.0;
        let cross = Path::new(|p| {
            p.move_to(Point::new(center.x - arm, center.y));
            p.line_to(Point::new(center.x + arm, center.y));
            p.move_to(Point::new(center.x, center.y - arm));
            p.line_to(Point::new(center.x, center.y + arm));
        });
        frame.stroke(&cross, Stroke::default().with_width(1.2).with_color(strong));
    }

    // 选中选框：外侧描边 + 四角实心手柄（手柄压在框角上，便于识别「可拖动」）
    if let Some(rect) = overlay.selection_box {
        let box_color = Color::from_rgba(strong.r, strong.g, strong.b, BOX_STROKE_ALPHA);
        let path = Path::rectangle(rect.position(), rect.size());
        frame.stroke(
            &path,
            Stroke::default()
                .with_width(BOX_STROKE_WIDTH)
                .with_color(box_color),
        );
        let half = BOX_HANDLE_SIZE / 2.0;
        let corners = [
            rect.position(),
            Point::new(rect.position().x + rect.width, rect.position().y),
            Point::new(rect.position().x, rect.position().y + rect.height),
            Point::new(
                rect.position().x + rect.width,
                rect.position().y + rect.height,
            ),
        ];
        for c in corners {
            let handle = Path::rectangle(
                Point::new(c.x - half, c.y - half),
                Size::new(BOX_HANDLE_SIZE, BOX_HANDLE_SIZE),
            );
            frame.fill(&handle, strong);
        }
    }

    if let Some(path) = &overlay.marquee_path {
        let fill = Color::from_rgba(
            marquee_color.r,
            marquee_color.g,
            marquee_color.b,
            MARQUEE_FILL_ALPHA,
        );
        let stroke_color = Color::from_rgba(
            marquee_color.r,
            marquee_color.g,
            marquee_color.b,
            MARQUEE_STROKE_ALPHA,
        );
        frame.fill(path, fill);
        frame.stroke(
            path,
            Stroke::default().with_width(1.0).with_color(stroke_color),
        );
    }

    Some(frame.into_geometry())
}

#[cfg(test)]
mod tests;
