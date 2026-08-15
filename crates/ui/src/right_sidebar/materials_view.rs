//! 右侧栏素材库面板视图渲染

use iced_core::{Alignment, Color, Length};
use iced_core::widget::text::Wrapping;
use iced_widget::{button, column, container, mouse_area, row, scrollable, text, tooltip};
use lumino_extras::i18n::{Language, MainTranslations, main_translations};
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

/// 单个素材项：列表仅显示名称；文件描述（名称/作者/位置/轨道数/来源）
/// 移入悬停提示悬浮面板
fn material_item<'a>(
    entry: &'a crate::right_sidebar::MaterialEntry,
    index: usize,
    language: Language,
) -> Element<'a> {
    let t = main_translations(language);

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

    let info_row = row![name_text]
        .spacing(4)
        .align_y(Alignment::Center)
        .width(Length::Fill);

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

    // 悬停提示悬浮面板：文件描述（名称/作者/位置/轨道数/来源，均带描述标头）
    // 显示在按钮左侧（右侧为素材列表区域，避免遮挡其他素材项）
    tooltip::Tooltip::new(item, tooltip_content(entry, t), tooltip::Position::Left)
        .style(tooltip_style)
        .into()
}

/// 悬停提示悬浮面板宽度（文本行固定宽度，超宽内容自动换行）
const TOOLTIP_WIDTH: f32 = 280.0;

/// 悬停提示悬浮面板内容：文件描述信息，每行均带描述标头
///
/// 显示项：
/// - 名称（metadata.project.name）
/// - 作者（metadata.project.author，素材导出时跟随工程设置面板署名；非空才显示）
/// - 位置（磁盘路径，仅本地素材）
/// - 轨道数（解析到音轨数时显示）
/// - 来源（内置 / 本地）
/// - 无效素材仅显示"素材无效"
fn tooltip_content<'a>(
    entry: &'a crate::right_sidebar::MaterialEntry,
    t: &'static MainTranslations,
) -> Element<'a> {
    let mut col = column![].spacing(2);

    if !entry.valid {
        col = col.push(text(t.material_invalid).size(10));
        return col.into();
    }

    // 名称
    col = col.push(tooltip_line(format!("{}{}", t.material_name_label, entry.name)));
    // 作者（跟随工程设置面板的作者栏目在 metadata 中署名）
    if !entry.author.is_empty() {
        col = col.push(tooltip_line(format!("{}{}", t.material_author_label, entry.author)));
    }
    // 位置（仅本地素材有磁盘路径；长路径自动换行）
    if let Some(path) = &entry.path {
        col = col.push(tooltip_line(format!(
            "{}{}",
            t.material_location_label,
            path.display()
        )));
    }
    // 轨道数
    if entry.track_count > 0 {
        col = col.push(tooltip_line(format!(
            "{}{}",
            t.material_track_label,
            entry.track_count
        )));
    }
    // 来源
    let source_label = match entry.source {
        MaterialSource::BuiltIn => t.material_section_builtin,
        MaterialSource::User => t.material_section_user,
    };
    col = col.push(tooltip_line(format!(
        "{}{}",
        t.material_source_label,
        source_label
    )));

    col.into()
}

/// 悬浮窗文本行：固定宽度 + 换行策略
///
/// iced 默认 `Wrapping::Word` 只按单词边界断行，磁盘路径等无空格长文本
/// 视为单个单词永不换行，会撑破悬浮窗——改用 `WordOrGlyph`：
/// 有空格按词断行，超长单词回退到字形级断行。
fn tooltip_line<'a>(content: String) -> Element<'a> {
    text(content)
        .size(10)
        .width(Length::Fixed(TOOLTIP_WIDTH))
        .wrapping(Wrapping::WordOrGlyph)
        .into()
}

/// Tooltip 样式：深色背景 + 浅色文字
fn tooltip_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(iced_core::Background::Color(Color::from_rgba(
            0.08, 0.08, 0.10, 0.96,
        ))),
        border: iced_core::Border::default().rounded(4),
        text_color: Some(Color::from_rgba(0.95, 0.95, 0.95, 1.0)),
        ..Default::default()
    }
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
    use std::path::PathBuf;

    use super::*;
    use crate::right_sidebar::material::MaterialEntry;

    fn make_entry(valid: bool, track_count: usize) -> MaterialEntry {
        MaterialEntry {
            name: "测试素材".into(),
            author: "测试作者".into(),
            source: MaterialSource::BuiltIn,
            path: None,
            data: None,
            multi_track: track_count > 1,
            track_count,
            valid,
            preview: None,
        }
    }

    #[test]
    fn test_material_item_builds_element() {
        let entry = make_entry(true, 4);
        let _element = material_item(&entry, 0, Language::ZhCn);
    }

    #[test]
    fn test_material_item_invalid_greyed() {
        let entry = make_entry(false, 0);
        let _element = material_item(&entry, 1, Language::ZhCn);
    }

    #[test]
    fn test_tooltip_content_builds_full_description() {
        // 有效素材：名称/作者/轨道数/来源均带描述标头；无路径不显示位置
        let entry = make_entry(true, 4);
        let t = main_translations(Language::ZhCn);
        let _element = tooltip_content(&entry, t);

        // 本地素材：额外显示位置（磁盘路径）
        let mut user_entry = MaterialEntry {
            path: Some(PathBuf::from("C:/Materials/demo.lmmaterial")),
            ..make_entry(true, 2)
        };
        user_entry.source = MaterialSource::User;
        let _element = tooltip_content(&user_entry, t);
    }

    #[test]
    fn test_tooltip_content_invalid_shows_invalid() {
        // 无效素材：仅显示"素材无效"
        let entry = make_entry(false, 0);
        let t = main_translations(Language::ZhCn);
        let _element = tooltip_content(&entry, t);
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
