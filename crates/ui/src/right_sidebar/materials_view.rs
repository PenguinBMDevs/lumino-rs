//! 右侧栏素材库面板视图渲染

use iced_core::{Alignment, Color, Length};
use iced_widget::{button, column, container, mouse_area, row, scrollable, text, tooltip};
use lumino_extras::i18n::{Language, main_translations};
use lumino_message::RightSidebarAction;

use crate::right_sidebar::core::{RESIZE_HANDLE_WIDTH, RightSidebar};
use crate::right_sidebar::material::MaterialSource;
use crate::{Element, Message, Theme, window};

/// 渲染素材库面板内容（标题 + 添加按钮 + 素材列表）
pub(super) fn panel<'a>(
    right_sidebar: &'a RightSidebar,
    language: Language,
    window: &'a window::Window,
) -> Element<'a> {
    let t = main_translations(language);

    let content_col = column![
        panel_header(t.material_library, window),
        add_button_section(right_sidebar, language),
        material_list(right_sidebar, language),
    ]
    .spacing(8)
    .padding(8)
    .width(Length::Fill);

    let content = container(scrollable(content_col).height(Length::Fill))
        .width(Length::Fixed(
            right_sidebar.panel_width - RESIZE_HANDLE_WIDTH,
        ))
        .height(Length::Fill)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style::default().background(palette.background.weakest.color)
        });

    content.into()
}

/// 面板标题文本（跟随主题：暗色白、亮色黑）
fn panel_header<'a>(title: &'a str, _window: &'a window::Window) -> Element<'a> {
    text(title)
        .size(14)
        .style(|theme: &Theme| text::Style {
            color: Some(theme.extended_palette().background.neutral.text),
        })
        .into()
}

/// 小节标题（跟随主题）
fn section_label<'a>(title: &'a str) -> Element<'a> {
    text(title)
        .size(12)
        .style(|theme: &Theme| text::Style {
            color: Some(theme.extended_palette().background.strong.text),
        })
        .into()
}

/// "添加素材"按钮 + 下拉菜单（从 web 下载 / 从本地选取）
fn add_button_section<'a>(right_sidebar: &'a RightSidebar, language: Language) -> Element<'a> {
    let t = main_translations(language);

    let add_btn = button(text(format!("+ {}", t.material_add)).size(13))
        .padding(6)
        .width(Length::Fill)
        .style(primary_button_style)
        .on_press(Message::RightSidebar(
            RightSidebarAction::MaterialAddClicked,
        ));

    let mut col = column![add_btn].spacing(2);
    if right_sidebar.materials.add_menu_open {
        // 下拉菜单项：从 web 下载（占位）
        let web_btn = button(text(t.material_download_web).size(12))
            .padding([4, 6])
            .width(Length::Fill)
            .style(menu_item_style)
            .on_press(Message::RightSidebar(
                RightSidebarAction::MaterialDownloadFromWeb,
            ));
        // 下拉菜单项：从本地选取
        let local_btn = button(text(t.material_import_local).size(12))
            .padding([4, 6])
            .width(Length::Fill)
            .style(menu_item_style)
            .on_press(Message::RightSidebar(
                RightSidebarAction::MaterialImportFromLocal,
            ));
        col = col.push(
            container(column![web_btn, local_btn].spacing(2))
                .padding(4)
                .style(|theme: &Theme| {
                    let palette = theme.extended_palette();
                    container::Style {
                        background: Some(palette.background.weak.color.into()),
                        border: iced_core::Border::default().rounded(4),
                        ..Default::default()
                    }
                }),
        );
    }
    col.into()
}

/// 素材列表（内置素材 + 本地素材分区）
fn material_list<'a>(right_sidebar: &'a RightSidebar, language: Language) -> Element<'a> {
    let t = main_translations(language);
    let materials = &right_sidebar.materials;

    if !materials.is_initialized() {
        return text(t.material_invalid)
            .size(12)
            .style(|theme: &Theme| text::Style {
                color: Some(theme.extended_palette().background.strong.text),
            })
            .into();
    }

    let mut col = column![].spacing(4);
    let mut builtin_items: Vec<Element<'a>> = Vec::new();
    let mut user_items: Vec<Element<'a>> = Vec::new();
    for (idx, entry) in materials.entries.iter().enumerate() {
        let item = material_item(entry, idx, language);
        match entry.source {
            MaterialSource::BuiltIn => builtin_items.push(item),
            MaterialSource::User => user_items.push(item),
        }
    }

    let builtin_empty = builtin_items.is_empty();
    let user_empty = user_items.is_empty();
    if !builtin_empty {
        col = col
            .push(section_label(t.material_section_builtin))
            .push(column(builtin_items).spacing(2));
    }
    if !user_empty {
        col = col
            .push(section_label(t.material_section_user))
            .push(column(user_items).spacing(2));
    }
    if builtin_empty && user_empty {
        col = col.push(
            text(t.material_section_user)
                .size(12)
                .style(|theme: &Theme| text::Style {
                    color: Some(theme.extended_palette().background.strong.text),
                }),
        );
    }
    col.into()
}

/// 单个素材项：名称 + 音轨数 + 来源标记；按下开始拖出（长按拖动到卷帘）
fn material_item<'a>(
    entry: &'a crate::right_sidebar::MaterialEntry,
    index: usize,
    language: Language,
) -> Element<'a> {
    let t = main_translations(language);
    let track_label = if entry.valid {
        if entry.track_count > 0 {
            // i18n 格式化字符串为静态字段，用 replace 填充（format! 需要字面量）
            t.material_tracks_fmt
                .replace("{}", &entry.track_count.to_string())
        } else {
            String::new()
        }
    } else {
        t.material_invalid.to_string()
    };
    let source_label = match entry.source {
        MaterialSource::BuiltIn => t.material_section_builtin,
        MaterialSource::User => t.material_section_user,
    };

    // 名称（有效实色 / 无效置灰）
    let name_text = text(&entry.name).size(12).style(move |theme: &Theme| {
        let palette = theme.extended_palette();
        let color = if entry.valid {
            palette.background.neutral.text
        } else {
            palette.background.strongest.text
        };
        text::Style { color: Some(color) }
    });

    let mut info_row = row![name_text]
        .spacing(4)
        .align_y(Alignment::Center)
        .width(Length::Fill);
    if !track_label.is_empty() {
        info_row = info_row.push(
            text(track_label)
                .size(10)
                .style(|theme: &Theme| text::Style {
                    color: Some(theme.extended_palette().background.strong.text),
                }),
        );
    }
    info_row = info_row.push(
        text(format!("· {source_label}"))
            .size(10)
            .style(|theme: &Theme| text::Style {
                color: Some(theme.extended_palette().background.strongest.text),
            }),
    );

    // 有效素材：按下即开始拖出（进入卷帘松手放置）；无效素材：不可交互置灰
    //
    // ⚠️ 不能用 button.on_press：iced 0.14 的 button 在【释放时】才触发 on_press
    // 且要求光标仍在按钮内——拖出素材（释放时鼠标已在卷帘）永不触发。
    // mouse_area.on_press 在【按下时】触发且不捕获后续事件，拖动链路完整。
    let content = container(info_row)
        .padding([6, 8])
        .width(Length::Fill)
        .style(move |theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style {
                background: Some(palette.background.weaker.color.into()),
                border: iced_core::Border::default().rounded(4),
                ..Default::default()
            }
        });

    let item: Element<'a> = if entry.valid {
        mouse_area(content)
            .on_press(Message::RightSidebar(
                RightSidebarAction::MaterialDragStarted(index),
            ))
            .into()
    } else {
        content.into()
    };

    // tooltip 显示在按钮左侧（右侧为素材列表区域，避免遮挡其他素材项）
    tooltip::Tooltip::new(item, entry.name.as_str(), tooltip::Position::Left)
        .style(|_theme: &Theme| container::Style {
            background: Some(iced_core::Background::Color(Color::from_rgba(
                0.08, 0.08, 0.10, 0.96,
            ))),
            border: iced_core::Border::default().rounded(4),
            text_color: Some(Color::from_rgba(0.95, 0.95, 0.95, 1.0)),
            ..Default::default()
        })
        .into()
}

/// 添加按钮样式（主色）
fn primary_button_style(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    let bg = match status {
        button::Status::Hovered | button::Status::Pressed => palette.primary.base.color,
        _ => palette.primary.weak.color,
    };
    button::Style {
        text_color: palette.background.base.text,
        border: iced_core::Border {
            radius: 4.0.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        ..Default::default()
    }
    .with_background(bg)
}

/// 下拉菜单项样式（普通按钮）
fn menu_item_style(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    let bg = match status {
        button::Status::Hovered | button::Status::Pressed => palette.background.base.color,
        _ => palette.background.weak.color,
    };
    button::Style {
        text_color: palette.background.base.text,
        border: iced_core::Border {
            radius: 4.0.into(),
            width: 0.0,
            color: Color::TRANSPARENT,
        },
        ..Default::default()
    }
    .with_background(bg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::right_sidebar::material::MaterialEntry;

    #[test]
    fn test_material_item_builds_element() {
        let entry = MaterialEntry {
            name: "测试素材".into(),
            source: MaterialSource::BuiltIn,
            path: None,
            data: None,
            multi_track: true,
            track_count: 4,
            valid: true,
            preview: None,
        };
        let _element = material_item(&entry, 0, Language::ZhCn);
    }

    #[test]
    fn test_material_item_invalid_greyed() {
        let entry = MaterialEntry {
            name: "损坏素材".into(),
            source: MaterialSource::User,
            path: None,
            data: None,
            multi_track: false,
            track_count: 0,
            valid: false,
            preview: None,
        };
        let _element = material_item(&entry, 1, Language::ZhCn);
    }

    #[test]
    fn test_panel_route_switch() {
        // 面板路由互斥切换（素材库面板 / I2M 面板）
        let mut sidebar = RightSidebar::new();
        assert!(!sidebar.panel_visible);
        sidebar.switch_panel(crate::right_sidebar::RightSidebarPanel::Materials);
        assert!(sidebar.panel_visible);
        assert!(sidebar.is_panel_active(crate::right_sidebar::RightSidebarPanel::Materials));
        assert!(!sidebar.is_panel_active(crate::right_sidebar::RightSidebarPanel::ImageToMidi));
    }
}
