//! DRAW-032B：区域着色调色板（最小浮层）
//!
//! 浮层只负责选色 / 退出；真正的 `AreaSpot` 写入与再生成在
//! `interaction::area_paint`。色号 0 = 清除（右键也会走 0）。

use crate::Editor;
use crate::grid::utils::content_bounds;
use iced_core::{Color, Point, Rectangle, Size};
use iced_widget::canvas::{self, Geometry, Path, Stroke};
use lumino_core::tr;
use lumino_message::EditorAction;
use lumino_ui_core::{Renderer, Theme};

/// 调色板尺寸
const SWATCH: f32 = 22.0;
const GAP: f32 = 3.0;
const PAD: f32 = 6.0;
const LABEL_H: f32 = 18.0;
const COLS: usize = 4;
const ROWS: usize = 4;
const CLOSE_SIZE: f32 = 16.0;

/// 调色板各矩形（画布坐标）
#[derive(Debug, Clone, Copy)]
pub struct AreaPaletteRects {
    /// 整个面板
    pub panel: Rectangle,
    /// 16 个色块（0..=15）
    pub swatches: [Rectangle; 16],
    /// 关闭按钮
    pub close: Rectangle,
    /// 当前色号
    pub active: u8,
}

/// 命中结果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AreaPaintHit {
    /// 选中某色号
    Colour(u8),
    /// 退出着色模式
    Close,
    /// 面板背景（消费点击，不落到画布）
    Panel,
}

/// 当前是否应显示调色板，以及各色块位置
pub fn palette_rects(editor: &Editor) -> Option<AreaPaletteRects> {
    if !editor.area_paint_active() {
        return None;
    }
    let anchor = editor.selection_box()?;
    let content = content_bounds(editor);
    let width = PAD * 2.0 + SWATCH * COLS as f32 + GAP * (COLS - 1) as f32;
    let height = PAD * 2.0 + LABEL_H + SWATCH * ROWS as f32 + GAP * (ROWS - 1) as f32;
    if content.width < width + 4.0 || content.height < height + 4.0 {
        return None;
    }

    let mut x = anchor.x;
    let mut y = anchor.y - height - 8.0;
    if y < content.y + 2.0 {
        y = anchor.y + anchor.height + 8.0;
    }
    x = x.clamp(
        content.x + 2.0,
        (content.x + content.width - width - 2.0).max(content.x + 2.0),
    );
    y = y.clamp(
        content.y + 2.0,
        (content.y + content.height - height - 2.0).max(content.y + 2.0),
    );
    let panel = Rectangle::new(Point::new(x, y), Size::new(width, height));
    let mut swatches = [Rectangle::new(Point::new(0.0, 0.0), Size::new(0.0, 0.0)); 16];
    for (slot, rect) in swatches.iter_mut().enumerate() {
        let col = slot % COLS;
        let row = slot / COLS;
        *rect = Rectangle::new(
            Point::new(
                x + PAD + col as f32 * (SWATCH + GAP),
                y + PAD + LABEL_H + row as f32 * (SWATCH + GAP),
            ),
            Size::new(SWATCH, SWATCH),
        );
    }
    let close = Rectangle::new(
        Point::new(x + width - PAD - CLOSE_SIZE, y + PAD),
        Size::new(CLOSE_SIZE, CLOSE_SIZE),
    );
    Some(AreaPaletteRects {
        panel,
        swatches,
        close,
        active: editor.area_paint_colour().unwrap_or(0),
    })
}

/// 命中调色板；点在面板外返回 `None`
pub fn hit(editor: &Editor, point: Point) -> Option<AreaPaintHit> {
    let rects = palette_rects(editor)?;
    if rects.close.contains(point) {
        return Some(AreaPaintHit::Close);
    }
    for (slot, rect) in rects.swatches.iter().enumerate() {
        if rect.contains(point) {
            return Some(AreaPaintHit::Colour(slot as u8));
        }
    }
    rects.panel.contains(point).then_some(AreaPaintHit::Panel)
}

/// 调色板命中 → 编辑器动作；`Panel` 只消费点击，不产生动作
pub fn action_for_hit(hit: AreaPaintHit) -> Option<EditorAction> {
    match hit {
        AreaPaintHit::Colour(colour) => Some(EditorAction::AreaPaintSetColour(colour)),
        AreaPaintHit::Close => Some(EditorAction::AreaPaintEnd),
        AreaPaintHit::Panel => None,
    }
}

/// 绘制调色板
pub fn draw(
    editor: &Editor,
    renderer: &Renderer,
    theme: &Theme,
    bounds: Rectangle,
) -> Option<Geometry<Renderer>> {
    let rects = palette_rects(editor)?;
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
    // canvas 文本必须用 Advanced shaping：Basic 不会走系统 CJK 回退。
    frame.fill_text(canvas::Text {
        content: tr!("区域着色").to_string(),
        position: Point::new(rects.panel.x + PAD, rects.panel.y + PAD - 1.0),
        max_width: rects.panel.width - PAD * 2.0 - CLOSE_SIZE,
        line_height: iced_core::text::LineHeight::Relative(1.0),
        size: iced_core::Pixels(11.0),
        color: palette.background.neutral.text,
        font: lumino_ui_core::font::ui_font(),
        align_x: iced_core::alignment::Horizontal::Left.into(),
        align_y: iced_core::alignment::Vertical::Top,
        shaping: iced_core::text::Shaping::Advanced,
    });
    frame.fill_text(canvas::Text {
        content: "×".to_string(),
        position: Point::new(rects.close.x + 2.0, rects.close.y - 1.0),
        max_width: CLOSE_SIZE,
        line_height: iced_core::text::LineHeight::Relative(1.0),
        size: iced_core::Pixels(14.0),
        color: palette.background.neutral.text,
        font: lumino_ui_core::font::ui_font(),
        align_x: iced_core::alignment::Horizontal::Center.into(),
        align_y: iced_core::alignment::Vertical::Top,
        shaping: iced_core::text::Shaping::Advanced,
    });

    for (slot, rect) in rects.swatches.iter().enumerate() {
        let colour = slot_color(slot as u8);
        let path = Path::rounded_rectangle(
            rect.position(),
            rect.size(),
            iced_core::border::Radius::from(3.0),
        );
        frame.fill(&path, colour);
        frame.stroke(
            &path,
            Stroke::default()
                .with_width(if slot as u8 == rects.active { 2.0 } else { 1.0 })
                .with_color(if slot as u8 == rects.active {
                    Color::WHITE
                } else {
                    palette.background.strong.color
                }),
        );
        if slot == 0 {
            // 0 = 清除：画一道斜杠，避免和“黑色”混淆
            frame.stroke(
                &Path::line(
                    Point::new(rect.x + 4.0, rect.y + rect.height - 4.0),
                    Point::new(rect.x + rect.width - 4.0, rect.y + 4.0),
                ),
                Stroke::default().with_width(2.0).with_color(Color::WHITE),
            );
        }
    }
    Some(frame.into_geometry())
}

/// 固定 16 色（0 = 清除/灰）
fn slot_color(slot: u8) -> Color {
    match slot {
        0 => Color::from_rgb8(70, 70, 70),
        1 => Color::from_rgb8(220, 60, 60),
        2 => Color::from_rgb8(230, 140, 50),
        3 => Color::from_rgb8(220, 200, 60),
        4 => Color::from_rgb8(80, 180, 90),
        5 => Color::from_rgb8(60, 180, 160),
        6 => Color::from_rgb8(70, 170, 220),
        7 => Color::from_rgb8(70, 110, 220),
        8 => Color::from_rgb8(130, 90, 220),
        9 => Color::from_rgb8(200, 90, 200),
        10 => Color::from_rgb8(230, 120, 170),
        11 => Color::from_rgb8(180, 120, 80),
        12 => Color::from_rgb8(150, 160, 70),
        13 => Color::from_rgb8(150, 210, 90),
        14 => Color::from_rgb8(100, 100, 180),
        _ => Color::from_rgb8(180, 180, 180),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::area_paint::AreaPaintState;
    use crate::tests::test_helpers::seed_notes;
    use lumino_editor_state::{DrawnShapeSource, shape_tool::ShapeKind};

    #[test]
    fn test_palette_hit_swatches_and_close() {
        let mut editor = Editor::new();
        seed_notes(&mut editor, 2, 1, &[]);
        editor.editor_state.tool = lumino_message::Tool::ShapeSelect;
        editor.editor_state.canvas.size_x = 800.0;
        editor.editor_state.canvas.size_y = 600.0;
        let drawn_id = editor.editor_state.shape_select.add(
            1,
            None,
            DrawnShapeSource::Shape {
                kind: ShapeKind::Rectangle,
                rect: (0.0, 60.0, 100.0, 64.0),
                shift_constrained: false,
                filled: true,
                apex_high: false,
            },
            Vec::new(),
        );
        editor
            .editor_state
            .shape_select
            .set_model_id(drawn_id, Some(1));
        editor.editor_state.shape_select.select_only(drawn_id);
        editor.area_paint = Some(AreaPaintState {
            drawn_id,
            model_id: 1,
            colour: 3,
        });
        let rects = palette_rects(&editor).expect("应显示调色板");
        let centre = |r: &Rectangle| Point::new(r.x + r.width * 0.5, r.y + r.height * 0.5);
        assert_eq!(
            hit(&editor, centre(&rects.swatches[3])),
            Some(AreaPaintHit::Colour(3))
        );
        assert_eq!(
            hit(&editor, centre(&rects.close)),
            Some(AreaPaintHit::Close)
        );
        assert_eq!(
            hit(
                &editor,
                Point::new(
                    rects.panel.x + 2.0,
                    rects.panel.y + rects.panel.height - 2.0
                )
            ),
            Some(AreaPaintHit::Panel)
        );
    }

    #[test]
    fn test_slot_colours_are_distinct() {
        let mut seen = std::collections::BTreeSet::new();
        for slot in 0..16 {
            let c = slot_color(slot);
            assert!(seen.insert((c.r.to_bits(), c.g.to_bits(), c.b.to_bits())));
        }
    }

    #[test]
    fn test_palette_hidden_without_area_mode() {
        let editor = Editor::new();
        assert!(palette_rects(&editor).is_none());
    }
}
