//! 事件浏览器 Canvas 绘制逻辑。
//!
//! 分页器/空表提示/通用文本与颜色绘制拆分为 `draw_parts`。

use iced_core::{Color, Font, Point, Rectangle, Size, alignment};
use iced_widget::canvas::{Frame, path::Builder};
use lumino_extras::i18n::MainTranslations;

use crate::Renderer;
use crate::Theme;
use crate::sidebar::event_browser::canvas::draw_parts::{
    colors, draw_empty_hint, draw_pager, draw_text,
};
use crate::sidebar::event_browser::canvas::popup::draw_popup;
use crate::sidebar::event_browser::canvas::{
    CanvasState, EventBrowserCanvas, HEADER_HEIGHT, MAX_TREE_WIDTH, MIN_TREE_WIDTH, ROW_HEIGHT,
    SPLITTER_WIDTH, TREE_INDENT,
};
use crate::sidebar::event_browser::detail::EventTableRow;
use crate::sidebar::event_browser::state::{ArchiveKey, SelectedItem, TreeItem};

/// 绘制主内容。
#[allow(clippy::too_many_arguments)]
pub(super) fn draw(
    frame: &mut Frame<Renderer>,
    theme: &Theme,
    bounds: iced_core::Rectangle,
    state: &CanvasState,
    canvas: &EventBrowserCanvas<'_>,
    tree_items: &[TreeItem],
    visible_tree_count: usize,
    rows: &[EventTableRow],
    page: usize,
    total_pages: usize,
) {
    fill_background(frame, bounds, theme);

    let tree_w = state.tree_width.clamp(MIN_TREE_WIDTH, MAX_TREE_WIDTH);
    let table_x = tree_w + SPLITTER_WIDTH;

    let font = canvas.font();

    // 左侧树
    draw_tree(
        frame,
        theme,
        state,
        canvas,
        tree_items,
        visible_tree_count,
        tree_w,
        state.scroll_y,
        font,
    );

    // 右侧表头（固定于视口顶部）
    // 使用 EventBrowserState 中的 selected_item（CanvasState 中的可能未同步）
    let selected_item = canvas.state.selected_item.as_ref();
    draw_table_header(
        frame,
        theme,
        table_x,
        bounds.width - table_x,
        selected_item,
        &state.column_widths,
        canvas.t,
        font,
    );

    // 右侧表格行
    draw_table_rows(
        frame,
        theme,
        table_x,
        bounds.width - table_x,
        canvas,
        rows,
        &state.column_widths,
        state.scroll_y,
        font,
    );

    // 列分隔线与表头底部分隔线
    draw_table_grid(
        frame,
        theme,
        table_x,
        bounds.width - table_x,
        state,
        bounds.height,
    );

    // 中间分隔条
    draw_splitter(frame, theme, tree_w, bounds.height);

    // 翻页器
    if total_pages > 1 {
        let table_content_h = HEADER_HEIGHT + rows.len() as f32 * ROW_HEIGHT;
        draw_pager(
            frame,
            theme,
            table_x,
            table_content_h,
            bounds.width - table_x,
            page,
            total_pages,
            font,
        );
    }

    // 空表加号
    if rows.is_empty() {
        draw_empty_hint(frame, theme, table_x, bounds.width - table_x, font);
    }

    // Popup 叠加层
    if let Some(popup) = &state.popup {
        draw_popup(frame, theme, popup, bounds, font);
    }
}

fn fill_background(frame: &mut Frame<Renderer>, bounds: Rectangle, theme: &Theme) {
    frame.fill_rectangle(
        Point::new(0.0, 0.0),
        bounds.size(),
        theme.extended_palette().background.base.color,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_tree(
    frame: &mut Frame<Renderer>,
    theme: &Theme,
    state: &CanvasState,
    canvas: &EventBrowserCanvas<'_>,
    tree_items: &[TreeItem],
    visible_count: usize,
    tree_w: f32,
    scroll_y: f32,
    font: Font,
) {
    let (header_bg, header_fg, _, _) = colors(theme);
    // 绘制表头
    frame.fill_rectangle(
        Point::new(0.0, 0.0),
        Size::new(tree_w, HEADER_HEIGHT),
        header_bg,
    );
    draw_text(
        frame,
        canvas.t.eb_archive,
        4.0,
        HEADER_HEIGHT * 0.5,
        header_fg,
        tree_w - 8.0,
        alignment::Horizontal::Left,
        font,
    );

    // 表头底部线（同时也是行高调整手柄指示器）
    draw_tree_header_bottom_line(frame, theme, tree_w);

    // 树项从 HEADER_HEIGHT 之后开始绘制，避免与表头重叠
    let row_h = state.tree_row_height;
    let scroll_remainder = scroll_y % row_h;
    let first = (scroll_y / row_h).floor() as usize;
    for (i, item) in tree_items
        .iter()
        .take(visible_count)
        .skip(first)
        .enumerate()
    {
        let y = HEADER_HEIGHT + i as f32 * row_h - scroll_remainder;
        draw_tree_item(frame, theme, canvas, item, tree_w, y, row_h, font);
    }
}

/// 绘制树表头底部分隔线（兼做行高调整手柄指示器）
fn draw_tree_header_bottom_line(frame: &mut Frame<Renderer>, theme: &Theme, tree_w: f32) {
    let (_, _, _, line_color) = colors(theme);
    let mut path = Builder::new();
    path.move_to(Point::new(0.0, HEADER_HEIGHT - 1.0));
    path.line_to(Point::new(tree_w, HEADER_HEIGHT - 1.0));
    frame.stroke(
        &path.build(),
        iced_widget::canvas::Stroke::default()
            .with_color(line_color)
            .with_width(1.0),
    );
}

fn draw_tree_item(
    frame: &mut Frame<Renderer>,
    theme: &Theme,
    canvas: &EventBrowserCanvas<'_>,
    item: &TreeItem,
    tree_w: f32,
    y: f32,
    row_h: f32,
    font: Font,
) {
    let (_, _, text_color, _) = colors(theme);
    let (name, depth, expandable, expanded) = match item {
        TreeItem::Root { name, key } => (
            name.as_str(),
            0u8,
            true,
            canvas.state.expanded_keys.contains(key),
        ),
        TreeItem::Leaf { name, depth, .. } => (name.as_str(), *depth, false, false),
        TreeItem::Track {
            name, id, depth, ..
        } => (
            name.as_str(),
            *depth,
            true,
            canvas.state.expanded_keys.contains(&ArchiveKey::Track(*id)),
        ),
    };

    let x = 4.0 + depth as f32 * TREE_INDENT + if expandable { 10.0 } else { 0.0 };
    let text_y = y + row_h * 0.5;

    if expandable {
        draw_triangle(
            frame,
            Point::new(x - 6.0, text_y),
            5.0,
            !expanded,
            text_color,
        );
    }

    let is_selected = matches!(item, TreeItem::Leaf { item, .. } if canvas.state.selected_item.as_ref() == Some(item));
    let bg = if is_selected {
        theme.extended_palette().primary.weak.color
    } else {
        Color::TRANSPARENT
    };
    if bg != Color::TRANSPARENT {
        frame.fill_rectangle(Point::new(0.0, y), Size::new(tree_w, row_h), bg);
    }

    draw_text(
        frame,
        name,
        x + 4.0,
        text_y,
        text_color,
        (tree_w - x - 4.0).max(0.0),
        alignment::Horizontal::Left,
        font,
    );
}

fn draw_triangle(
    frame: &mut Frame<Renderer>,
    center: Point,
    size: f32,
    point_right: bool,
    color: Color,
) {
    let mut path = Builder::new();
    let half = size * 0.5;
    if point_right {
        path.move_to(Point::new(center.x - half, center.y - half));
        path.line_to(Point::new(center.x + half, center.y));
        path.line_to(Point::new(center.x - half, center.y + half));
    } else {
        path.move_to(Point::new(center.x - half, center.y - half));
        path.line_to(Point::new(center.x + half, center.y - half));
        path.line_to(Point::new(center.x, center.y + half));
    }
    path.close();
    frame.fill(&path.build(), color);
}

fn draw_table_header(
    frame: &mut Frame<Renderer>,
    theme: &Theme,
    table_x: f32,
    table_w: f32,
    selected_item: Option<&SelectedItem>,
    column_widths: &[f32],
    t: &MainTranslations,
    font: Font,
) {
    let (header_bg, header_fg, _, line_color) = colors(theme);
    frame.fill_rectangle(
        Point::new(table_x, 0.0),
        Size::new(table_w, HEADER_HEIGHT),
        header_bg,
    );

    if let Some(item) = selected_item {
        let headers = crate::sidebar::event_browser::detail::headers(item, t);
        let mut x = table_x + 4.0;
        for (i, (title, _)) in headers.iter().enumerate() {
            let w = column_widths.get(i).copied().unwrap_or(60.0);
            draw_text(
                frame,
                title,
                x,
                HEADER_HEIGHT * 0.5,
                header_fg,
                w - 8.0,
                alignment::Horizontal::Left,
                font,
            );
            x += w;
        }
    }

    // 表头底部线
    let mut path = Builder::new();
    path.move_to(Point::new(table_x, HEADER_HEIGHT - 1.0));
    path.line_to(Point::new(table_x + table_w, HEADER_HEIGHT - 1.0));
    frame.stroke(
        &path.build(),
        iced_widget::canvas::Stroke::default()
            .with_color(line_color)
            .with_width(1.0),
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_table_rows(
    frame: &mut Frame<Renderer>,
    theme: &Theme,
    table_x: f32,
    table_w: f32,
    canvas: &EventBrowserCanvas<'_>,
    rows: &[EventTableRow],
    column_widths: &[f32],
    scroll_y: f32,
    font: Font,
) {
    let (_, _, text_color, line_color) = colors(theme);
    let first = (scroll_y / ROW_HEIGHT).floor() as usize;
    let palette = theme.extended_palette();

    for (offset, row) in rows.iter().skip(first).enumerate() {
        let y = HEADER_HEIGHT + offset as f32 * ROW_HEIGHT;
        let selected = canvas.state.selected_ticks.contains(&row.tick);
        let bg = if selected {
            palette.primary.weak.color
        } else if offset % 2 == 0 {
            palette.background.weak.color
        } else {
            palette.background.base.color
        };
        frame.fill_rectangle(Point::new(table_x, y), Size::new(table_w, ROW_HEIGHT), bg);

        let mut x = table_x + 4.0;
        for (i, cell) in row.cells.iter().enumerate() {
            let w = column_widths.get(i).copied().unwrap_or(60.0);
            draw_text(
                frame,
                cell,
                x,
                y + ROW_HEIGHT * 0.5,
                text_color,
                w - 8.0,
                alignment::Horizontal::Left,
                font,
            );
            x += w;
        }

        // 行底部分隔线
        let mut path = Builder::new();
        path.move_to(Point::new(table_x, y + ROW_HEIGHT - 1.0));
        path.line_to(Point::new(table_x + table_w, y + ROW_HEIGHT - 1.0));
        frame.stroke(
            &path.build(),
            iced_widget::canvas::Stroke::default()
                .with_color(line_color)
                .with_width(1.0),
        );
    }
}

fn draw_table_grid(
    frame: &mut Frame<Renderer>,
    theme: &Theme,
    table_x: f32,
    _table_w: f32,
    state: &CanvasState,
    canvas_h: f32,
) {
    let (_, _, _, line_color) = colors(theme);
    let mut path = Builder::new();
    let mut x = table_x;
    for &w in state
        .column_widths
        .iter()
        .take(state.column_widths.len().saturating_sub(1))
    {
        x += w;
        path.move_to(Point::new(x, 0.0));
        path.line_to(Point::new(x, canvas_h));
    }
    frame.stroke(
        &path.build(),
        iced_widget::canvas::Stroke::default()
            .with_color(line_color)
            .with_width(1.0),
    );
}

fn draw_splitter(frame: &mut Frame<Renderer>, theme: &Theme, x: f32, height: f32) {
    let (_, _, _, line_color) = colors(theme);
    let mut path = Builder::new();
    path.move_to(Point::new(x + SPLITTER_WIDTH * 0.5, 0.0));
    path.line_to(Point::new(x + SPLITTER_WIDTH * 0.5, height));
    frame.stroke(
        &path.build(),
        iced_widget::canvas::Stroke::default()
            .with_color(line_color.scale_alpha(0.5))
            .with_width(SPLITTER_WIDTH),
    );
}
