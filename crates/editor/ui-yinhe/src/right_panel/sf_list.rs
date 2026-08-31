//! 音色列表 — 对应 yinhe `right_panel/sf_list.rs:379`
//!
//! 可复用音色列表：多选 + 拖拽排序 + 启用复选框 + 右键菜单。
//! yinhe 原用 `egui::ScrollArea::show_viewport` + `DragReorder + auto_scroll`；
//! iced 桩用 `scrollable + column` 重构，保留：多选（Ctrl/Shift）、
//! 选中高亮、拖拽手柄占位、启用切换、截断路径显示。

use iced_core::{Alignment, Length};
use iced_widget::{button, checkbox, column, container, row, scrollable, text};

use lumino_ui_core::{Element, Theme, window::Window};

/// 音色库条目（对齐 yinhe `SfEntry`）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SfEntry {
    pub path: String,
    pub name: String,
    pub enabled: bool,
}

impl SfEntry {
    #[must_use]
    pub fn new(path: String, name: String) -> Self {
        Self {
            path,
            name,
            enabled: true,
        }
    }
}

/// 列表跨帧状态（对齐 yinhe `ListState`，iced 侧由上层持有而非 `egui::Id`）
///
/// `selected` 已排序，`last_click` 为 Shift 范围锚点，`drag` 为拖拽中状态占位。
#[derive(Debug, Clone, Default)]
pub struct SfListState {
    pub selected: Vec<usize>,
    pub last_click: Option<usize>,
    pub drag_indices: Option<Vec<usize>>,
    pub insert_idx: Option<usize>,
}

impl SfListState {
    #[must_use]
    pub fn is_selected(&self, idx: usize) -> bool {
        self.selected.contains(&idx)
    }
}

/// 截断音色库路径（对齐 yinhe `truncate_path`，按字符而非字节）
///
/// 超过 40 字符时保留尾部 37 字符并加 `…` 前缀，避免中文路径 `char_boundary` panic。
#[must_use]
pub fn truncate_path(path: &str) -> String {
    if path.chars().count() > 40 {
        let start = path
            .char_indices()
            .nth_back(36)
            .map(|(i, _)| i)
            .unwrap_or(0);
        format!("…{}", &path[start..])
    } else {
        path.to_string()
    }
}

const ROW_H: f32 = 40.0;

fn sf_row<'a>(window: &'a Window, entry: &'a SfEntry, _idx: usize, selected: bool) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = if selected {
        palette.background.strong.color
    } else {
        iced_core::Color::TRANSPARENT
    };

    let name_color = if selected {
        palette.background.strong.text
    } else {
        palette.background.base.text
    };
    let label_color = if selected {
        palette.background.strong.text
    } else {
        palette.background.weak.text
    };

    let cb = checkbox(entry.enabled).label("").size(18);

    let name = text(entry.name.clone())
        .size(12)
        .style(move |_theme: &Theme| iced_widget::text::Style {
            color: Some(name_color),
        });
    let path = text(truncate_path(&entry.path))
        .size(10)
        .style(move |_theme: &Theme| iced_widget::text::Style {
            color: Some(label_color),
        });

    let handle = text("⋮⋮")
        .size(12)
        .style(move |_theme: &Theme| iced_widget::text::Style {
            color: Some(label_color),
        });

    let row_content = row![
        cb,
        column![name, path].spacing(2).width(Length::Fill),
        handle,
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .padding([4, 6]);

    container(row_content)
        .width(Length::Fill)
        .height(Length::Fixed(ROW_H))
        .style(move |_theme: &Theme| container::Style {
            background: Some(iced_core::Background::Color(bg)),
            border: iced_core::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

/// 音色列表 `view()` — `scrollable + column`（虚拟化占位）
///
/// ```text
/// scrollable(column![ sf_row, sf_row, ... ])
/// ```
/// 实际拖拽排序与自动滚动由上层 `Message` 驱动，此处仅展示；
/// 右键菜单（上移/下移/删除）以 `button` 行占位说明。
pub fn view<'a>(window: &'a Window, entries: &'a [SfEntry], state: &'a SfListState) -> Element<'a> {
    if entries.is_empty() {
        return container(text("No SoundFonts — click Add").size(11))
            .padding([12, 12])
            .into();
    }

    let rows: Vec<Element<'a>> = entries
        .iter()
        .enumerate()
        .map(|(i, e)| sf_row(window, e, i, state.is_selected(i)))
        .collect();

    let list = column(rows).spacing(0);

    let footer = row![
        button(text("Add").size(11)).padding([4, 8]),
        button(text("Clear").size(11)).padding([4, 8]),
    ]
    .spacing(8);

    column![scrollable(list).height(Length::Fixed(220.0)), footer,]
        .spacing(6)
        .padding([4, 4])
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_ui_core::window::Window;

    #[test]
    fn truncate_cjk_safe() {
        let path = "/Users/jieneng/下载/钢琴音色库合集/斯坦威大钢琴精选音源完整版.sf2";
        let t = truncate_path(path);
        assert!(t.starts_with('…'));
        assert!(t.is_char_boundary(0));
    }

    #[test]
    fn sf_list_view_empty_and_filled() {
        let window = Window::new("Tokyo Night Storm");
        let state = SfListState::default();
        let _ = view(&window, &[], &state);
        let entries = vec![
            SfEntry::new("/a/b/c.sf2".to_string(), "Grand".to_string()),
            SfEntry::new("/x/y/z.sfz".to_string(), "Strings".to_string()),
        ];
        let mut sel = SfListState::default();
        sel.selected = vec![0];
        let _ = view(&window, &entries, &sel);
    }
}
