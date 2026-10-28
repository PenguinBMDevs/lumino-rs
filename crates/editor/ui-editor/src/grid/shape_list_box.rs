//! DRAW-041B/032B：Bake 后保留的 ShapeStore 模型列表
//!
//! 普通 Bake 仍不登记 DrawnShape（保持 DRAW-041 语义），列表是这些只读模型的
//! 唯一入口；点击一条会进入 032B 区域着色。载入工程后没有本次会话音符指纹的
//! 模型暂不可编辑（避免删不掉旧音符造成重复）。

use crate::Editor;
use crate::grid::utils::content_bounds;
use iced_core::{Point, Rectangle, Size};
use iced_widget::canvas::{self, Geometry, Path, Stroke};
use lumino_core::tr;
use lumino_message::EditorAction;
use lumino_ui_core::{Renderer, Theme};

const PANEL_W: f32 = 230.0;
const ROW_H: f32 = 24.0;
const PAD: f32 = 8.0;
const TITLE_H: f32 = 20.0;
const CLOSE: f32 = 16.0;

/// 列表矩形
#[derive(Debug, Clone)]
pub struct ShapeListRects {
    /// 面板
    pub panel: Rectangle,
    /// 行：模型 id + 矩形
    pub rows: Vec<(u64, Rectangle, String, bool)>,
    /// 关闭按钮
    pub close: Rectangle,
}

/// 列表命中
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeListHit {
    /// 选中模型
    Row(u64),
    /// 关闭列表
    Close,
    /// 面板空白
    Panel,
}

/// 当前应显示的列表矩形；无模型/未展开时返回 `None`
pub fn shape_list_rects(editor: &Editor) -> Option<ShapeListRects> {
    if !editor.shape_list_open {
        return None;
    }
    let track = editor.editor_state.data.current_track;
    let mut entries: Vec<(u64, String, bool)> = editor
        .editor_state
        .shape_store
        .iter()
        .filter(|entry| entry.is_visible() && entry.model.track as usize == track)
        .map(|entry| {
            let name = if entry.model.name.is_empty() {
                format!("{} #{}", tr!("形状"), entry.model.id)
            } else {
                entry.model.name.clone()
            };
            // 载入工程后 notes 为空，但再生成结果能精确定位时仍可接管。
            (
                entry.model.id,
                name,
                editor.can_area_paint_model(entry.model.id),
            )
        })
        .collect();
    entries.sort_by_key(|(id, _, _)| *id);
    if entries.is_empty() {
        return None;
    }
    let content = content_bounds(editor);
    let height = PAD * 2.0 + TITLE_H + entries.len() as f32 * ROW_H;
    if content.width < PANEL_W + 8.0 || content.height < height + 8.0 {
        return None;
    }
    let panel = Rectangle::new(
        Point::new(content.x + 12.0, content.y + 12.0),
        Size::new(PANEL_W, height),
    );
    let rows = entries
        .into_iter()
        .enumerate()
        .map(|(index, (id, name, editable))| {
            (
                id,
                Rectangle::new(
                    Point::new(
                        panel.x + PAD,
                        panel.y + PAD + TITLE_H + index as f32 * ROW_H,
                    ),
                    Size::new(PANEL_W - PAD * 2.0, ROW_H),
                ),
                name,
                editable,
            )
        })
        .collect();
    let close = Rectangle::new(
        Point::new(panel.x + panel.width - PAD - CLOSE, panel.y + PAD),
        Size::new(CLOSE, CLOSE),
    );
    Some(ShapeListRects { panel, rows, close })
}

/// 命中列表
pub fn hit(editor: &Editor, point: Point) -> Option<ShapeListHit> {
    let rects = shape_list_rects(editor)?;
    if rects.close.contains(point) {
        return Some(ShapeListHit::Close);
    }
    for (id, rect, _, _) in &rects.rows {
        if rect.contains(point) {
            return Some(ShapeListHit::Row(*id));
        }
    }
    rects.panel.contains(point).then_some(ShapeListHit::Panel)
}

/// 左键命中列表：`None` = 列表未展开；`Some(None)` = 面板空白消费；
/// `Some(Some(action))` = 选择/关闭动作。
pub fn press_action(editor: &Editor, point: Point) -> Option<Option<EditorAction>> {
    if !editor.shape_list_open {
        return None;
    }
    Some(match hit(editor, point) {
        Some(ShapeListHit::Row(id)) => Some(EditorAction::ShapeListPick(id)),
        Some(ShapeListHit::Close) => Some(EditorAction::ShapeListClose),
        Some(ShapeListHit::Panel) => None,
        None => Some(EditorAction::ShapeListClose),
    })
}

/// 绘制列表
pub fn draw(
    editor: &Editor,
    renderer: &Renderer,
    theme: &Theme,
    bounds: Rectangle,
) -> Option<Geometry<Renderer>> {
    let rects = shape_list_rects(editor)?;
    let palette = theme.extended_palette();
    let mut frame = canvas::Frame::new(renderer, bounds.size());
    let panel_path = Path::rounded_rectangle(
        rects.panel.position(),
        rects.panel.size(),
        iced_core::border::Radius::from(6.0),
    );
    frame.fill(&panel_path, palette.background.weakest.color);
    frame.stroke(
        &panel_path,
        Stroke::default()
            .with_width(1.0)
            .with_color(palette.background.strong.color),
    );
    frame.fill_text(canvas::Text {
        content: tr!("形状列表").to_string(),
        position: Point::new(rects.panel.x + PAD, rects.panel.y + PAD - 1.0),
        max_width: rects.panel.width - PAD * 2.0 - CLOSE,
        line_height: iced_core::text::LineHeight::Relative(1.0),
        size: iced_core::Pixels(12.0),
        color: palette.background.neutral.text,
        font: lumino_ui_core::font::ui_font(),
        align_x: iced_core::alignment::Horizontal::Left.into(),
        align_y: iced_core::alignment::Vertical::Top,
        shaping: iced_core::text::Shaping::Advanced,
    });
    frame.fill_text(canvas::Text {
        content: "×".to_string(),
        position: Point::new(rects.close.x + 2.0, rects.close.y - 1.0),
        max_width: CLOSE,
        line_height: iced_core::text::LineHeight::Relative(1.0),
        size: iced_core::Pixels(14.0),
        color: palette.background.neutral.text,
        font: lumino_ui_core::font::ui_font(),
        align_x: iced_core::alignment::Horizontal::Center.into(),
        align_y: iced_core::alignment::Vertical::Top,
        shaping: iced_core::text::Shaping::Advanced,
    });
    for (index, (_, rect, name, editable)) in rects.rows.iter().enumerate() {
        if index % 2 == 0 {
            let bg = Path::rectangle(rect.position(), rect.size());
            frame.fill(&bg, palette.background.weak.color);
        }
        frame.fill_text(canvas::Text {
            content: if *editable {
                name.clone()
            } else {
                format!("{name}（只读）")
            },
            position: Point::new(rect.x + 6.0, rect.y + 4.0),
            max_width: rect.width - 12.0,
            line_height: iced_core::text::LineHeight::Relative(1.0),
            size: iced_core::Pixels(12.0),
            color: if *editable {
                palette.background.neutral.text
            } else {
                palette.background.strong.text
            },
            font: lumino_ui_core::font::ui_font(),
            align_x: iced_core::alignment::Horizontal::Left.into(),
            align_y: iced_core::alignment::Vertical::Top,
            shaping: iced_core::text::Shaping::Advanced,
        });
    }
    Some(frame.into_geometry())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::test_helpers::seed_notes;
    use lumino_core::shape::{ShapeGeometry, ShapeKind, ShapeModel};
    use lumino_editor_state::ShapeNote;

    #[test]
    fn test_list_hidden_without_open_flag() {
        let editor = Editor::new();
        assert!(shape_list_rects(&editor).is_none());
    }

    #[test]
    fn test_list_row_hit() {
        let mut editor = Editor::new();
        seed_notes(&mut editor, 2, 1, &[]);
        // 形状列表不再依赖鼠标工具：画刷 √ 后也应能打开。
        editor.editor_state.tool = lumino_message::Tool::Brush;
        editor.editor_state.canvas.size_x = 800.0;
        editor.editor_state.canvas.size_y = 600.0;
        let mut model = ShapeModel::new(0, 1, ShapeKind::Curve, 1.0);
        model.geometry = ShapeGeometry::from_points([(0.0, 60.0), (100.0, 64.0)]);
        model.name = "test".to_string();
        let id = editor.editor_state.shape_store.insert(model, 0);
        editor.editor_state.shape_store.assign_notes(
            id,
            vec![ShapeNote {
                track: 1,
                tick: 0.0,
                key: 60,
                length: 100.0,
                velocity: 100,
                channel: 0,
            }],
        );
        editor.shape_list_open = true;
        let rects = shape_list_rects(&editor).expect("应显示列表");
        let row = &rects.rows[0];
        assert_eq!(
            hit(&editor, Point::new(row.1.x + 4.0, row.1.y + 4.0)),
            Some(ShapeListHit::Row(id))
        );
        assert_eq!(row.2, "test");
        assert!(row.3, "本次会话模型应可编辑");
    }
}
