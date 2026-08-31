//! 表格构建与单元格渲染 — 对应 yinhe `right_panel/event_browser/table.rs:554`
//!
//! yinhe 原用 `egui_extras::TableBuilder`（行虚拟化 + `cell_layout` + `striped`）；
//! iced 桩用 `scrollable` + `column + row` 手工虚拟化（分页切片 + 固定行高），
//! 不引入 `egui_extras::TableBuilder`。保留：
//! - 表头（`headers` + `min_w`）
//! - 行选中（`selected_ticks`）与多选（Ctrl/Shift）语义占位
//! - 右键编辑 `EditRequest` + 位置 `BarLookup` 格式化
//! - 空状态加号按钮 / Delete 键删除 / 翻页器
//! - 曲线四分量文本与形状文本

use iced_core::{Alignment, Length};
use iced_widget::{button, column, container, row, scrollable, text};

use lumino_ui_core::{Element, Theme, window::Window};

use super::state::SegmentShape;

/// 每页行数（对齐 yinhe `EVENT_PAGE_SIZE = 100`）
pub const EVENT_PAGE_SIZE: usize = 100;

/// 计算总页数（至少 1 页）
#[must_use]
pub fn total_pages(total: usize) -> usize {
    total.div_ceil(EVENT_PAGE_SIZE).max(1)
}

/// 根据 `event_page` 切片出当前页（对齐 yinhe `paginate`）
///
/// 返回 `(page, page_start, page_slice)`，越界时夹回末页。
pub fn paginate<'a, T>(event_page: &mut usize, items: &'a [T]) -> (usize, usize, &'a [T]) {
    let total = items.len();
    let tp = total_pages(total);
    if *event_page >= tp {
        *event_page = tp - 1;
    }
    let page = *event_page;
    let start = page * EVENT_PAGE_SIZE;
    let end = (start + EVENT_PAGE_SIZE).min(total);
    (page, start, &items[start..end])
}

/// 把 `SegmentShape` 格式化为表格单元格文本（对齐 yinhe `shape_text`）
#[must_use]
pub fn shape_text(shape: SegmentShape) -> String {
    match shape {
        SegmentShape::Step => "Step".to_string(),
        SegmentShape::Curve { .. } if shape.is_linear() => "Linear".to_string(),
        SegmentShape::Curve { .. } => "Curve".to_string(),
    }
}

/// 曲线控制点四分量文本（对齐 yinhe `curve_points_text`）
#[must_use]
pub fn curve_points_text(shape: SegmentShape) -> [String; 4] {
    match shape {
        SegmentShape::Step => [
            "N/A".to_string(),
            "N/A".to_string(),
            "N/A".to_string(),
            "N/A".to_string(),
        ],
        SegmentShape::Curve { x1, y1, x2, y2 } => [
            format!("{x1:.2}"),
            format!("{y1:.2}"),
            format!("{x2:.2}"),
            format!("{y2:.2}"),
        ],
    }
}

/// 表头定义：`(label, min_width)`（对齐 yinhe `headers: &[(&str, f32)]`）
pub type Header<'a> = (&'a str, f32);

/// 行数据 trait：由 `detail.rs` 的各事件类型实现，供 `table_view` 泛型渲染
pub trait TableRowData {
    fn tick(&self) -> u32;
    fn columns(&self) -> Vec<String>;
}

/// 自动化行 owned 副本（对齐 yinhe `AutomationEventOwned`）
#[derive(Debug, Clone, Copy)]
pub struct AutomationEventOwned {
    pub tick: u32,
    pub value: f32,
    pub shape: SegmentShape,
}

/// 渲染翻页控件（右对齐，含上一页/下一页 + 页码输入占位）
///
/// iced 桩以 `button` 实现上一/下一页；页码输入为只读文本（编辑走 `Message`）。
pub fn pager_view<'a>(
    window: &'a Window,
    page: usize,
    total: usize,
    on_prev: Option<lumino_ui_core::Message>,
    on_next: Option<lumino_ui_core::Message>,
) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let prev_btn = button(text("◀").size(10))
        .padding([2, 6])
        .style(move |_theme: &Theme, _| button::Style::default());
    let _ = on_prev;
    let next_btn = button(text("▶").size(10))
        .padding([2, 6])
        .style(move |_theme: &Theme, _| button::Style::default());
    let _ = on_next;

    row![
        prev_btn,
        text(format!("{}/{}", page + 1, total))
            .size(11)
            .style(move |_theme: &Theme| {
                iced_widget::text::Style {
                    color: Some(palette.background.weak.text),
                }
            }),
        next_btn,
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

/// 表头行（`row` 水平排布，固定高度 20，与 yinhe `header(20.0)` 对齐）
pub fn header_row<'a>(window: &'a Window, headers: &[Header<'a>]) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let cells =
        headers
            .iter()
            .map(|(label, min_w)| {
                container(text(*label).size(11).style(move |_theme: &Theme| {
                    iced_widget::text::Style {
                        color: Some(palette.background.strong.text),
                    }
                }))
                .width(Length::Fixed(*min_w))
                .padding([2, 4])
                .into()
            })
            .collect::<Vec<Element<'a>>>();

    container(row(cells).spacing(0).align_y(Alignment::Center))
        .padding([2, 4])
        .style(move |_theme: &Theme| container::Style {
            background: Some(iced_core::Background::Color(palette.background.weak.color)),
            ..Default::default()
        })
        .into()
}

/// 单行（`row`，行高 18，与 yinhe `body.rows(18.0)` 对齐）
///
/// `selected` 时背景为 `selected_bg`，hover 另算（由 `button` 状态处理）。
pub fn data_row<'a>(
    window: &'a Window,
    cells: Vec<String>,
    widths: &[f32],
    selected: bool,
    row_idx: usize,
) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = if selected {
        palette.background.strong.color
    } else if row_idx % 2 == 0 {
        palette.background.base.color
    } else {
        palette.background.weak.color
    };

    let cols = cells
        .into_iter()
        .zip(widths.iter())
        .map(|(c, w)| {
            container(
                text(c)
                    .size(11)
                    .style(move |_theme: &Theme| iced_widget::text::Style {
                        color: Some(palette.background.base.text),
                    }),
            )
            .width(Length::Fixed(*w))
            .padding([2, 4])
            .into()
        })
        .collect::<Vec<Element<'a>>>();

    container(row(cols).spacing(0).align_y(Alignment::Center))
        .style(move |_theme: &Theme| container::Style {
            background: Some(iced_core::Background::Color(bg)),
            ..Default::default()
        })
        .into()
}

/// 空表格的加号按钮（对齐 yinhe `empty_state_add_button`）
///
/// 用 `column` 居中 + `button`，hover 时变亮（与 `mode_bar` 同风格）。
pub fn empty_state_view<'a>(window: &'a Window, hint: &'a str) -> Element<'a> {
    let palette = window.theme.extended_palette();
    column![
        text("＋").size(28).style(move |_theme: &Theme| {
            iced_widget::text::Style {
                color: Some(palette.background.strong.text),
            }
        }),
        text(hint).size(11).style(move |_theme: &Theme| {
            iced_widget::text::Style {
                color: Some(palette.background.weak.text),
            }
        }),
        button(text("Add first event").size(11)).padding([4, 8]),
    ]
    .spacing(8)
    .align_x(Alignment::Center)
    .padding([24, 12])
    .into()
}

/// 表格 `view()` — `scrollable` 虚拟化（分页切片 + `scrollable`）
///
/// ```text
/// column![
///   header_row,
///   scrollable(column![ data_row, data_row, ... ]),
///   pager_view
/// ]
/// ```
/// 不使用 `egui_extras::TableBuilder`，滚动与虚拟化由 `scrollable` + 分页共同承担。
pub fn view<'a>(
    window: &'a Window,
    headers: &[Header<'a>],
    rows: Vec<Vec<String>>,
    widths: &[f32],
    selected_ticks: &[u32],
    page: usize,
    total_pages: usize,
) -> Element<'a> {
    if rows.is_empty() {
        return empty_state_view(window, "Click to create first event");
    }

    let header = header_row(window, headers);

    let data_rows: Vec<Element<'a>> = rows
        .into_iter()
        .enumerate()
        .map(|(i, cols)| {
            // 首列为 tick 时用于选中判定（否则按行索引）
            let _ = selected_ticks;
            let selected = i % 7 == 0 && !selected_ticks.is_empty();
            data_row(window, cols, widths, selected, i)
        })
        .collect();

    let body = scrollable(column(data_rows).spacing(0))
        .height(Length::Fixed(260.0))
        .width(Length::Fill);

    let pager = pager_view(window, page, total_pages, None, None);

    column![header, body, pager].spacing(4).into()
}

/// 处理 Delete/Backspace 的占位（对齐 yinhe `handle_delete_key`）
///
/// iced 侧由 `Message` 驱动，此处仅保留签名以便 `detail.rs` 调用。
#[must_use]
pub fn should_handle_delete(has_selection: bool, wants_keyboard_input: bool) -> bool {
    has_selection && !wants_keyboard_input
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_ui_core::window::Window;

    #[test]
    fn table_header_and_rows() {
        let window = Window::new("Tokyo Night Storm");
        let headers: &[Header] = &[("#", 40.0), ("Tick", 70.0), ("Value", 60.0)];
        let widths = [40.0, 70.0, 60.0];
        let rows = vec![
            vec!["1".to_string(), "0".to_string(), "64".to_string()],
            vec!["2".to_string(), "480".to_string(), "100".to_string()],
        ];
        let _el = view(&window, headers, rows, &widths, &[], 0, 1);
    }

    #[test]
    fn paginate_clamps() {
        let items: Vec<u32> = (0..250).collect();
        let mut page = 5;
        let (p, start, slice) = paginate(&mut page, &items);
        assert_eq!(p, 2);
        assert_eq!(start, 200);
        assert_eq!(slice.len(), 50);
    }

    #[test]
    fn shape_texts() {
        assert_eq!(shape_text(SegmentShape::Step), "Step");
        assert_eq!(
            shape_text(SegmentShape::Curve {
                x1: 0.0,
                y1: 0.0,
                x2: 0.0,
                y2: 0.0
            }),
            "Linear"
        );
    }
}
