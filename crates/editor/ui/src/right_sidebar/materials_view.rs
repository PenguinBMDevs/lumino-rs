//! 右侧栏素材库面板视图渲染
//!
//! 悬停提示面板见 `tip` 子模块，单元测试见 `tests` 子模块。

#[cfg(test)]
mod tests;
mod tip;

use tip::*;

use iced_core::{Alignment, Color, Length};
use iced_widget::{
    Stack, button, column, container, mouse_area, row, scrollable, text, text_input, tooltip,
};
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

    // 面板级鼠标追踪：on_move 上报鼠标相对本面板的局部坐标，
    // 作为右键菜单的弹出位置数据源（无需窗口坐标系换算）。
    //
    // 右键兜底：素材项的 mouse_area 会先捕获右键事件（on_right_press 打开
    // 该素材的菜单），空白区域右键落空后由本层兜底关闭菜单——保证菜单
    // 只会在素材项上右键时出现，空白处右键不会打开菜单。
    let content: Element<'a> = mouse_area(content)
        .on_move(|pos| Message::RightSidebar(RightSidebarAction::MaterialCursorMoved(pos.x, pos.y)))
        .on_right_press(Message::RightSidebar(
            RightSidebarAction::MaterialContextMenuClosed,
        ))
        .into();

    // 右键菜单叠加：菜单打开时在面板上覆盖关闭背景 + 悬浮菜单
    let materials = &right_sidebar.materials;
    let Some(target) = materials.context_menu_target else {
        return content;
    };
    if materials.entries.get(target).is_none() {
        return content;
    }
    // 重命名/删除仅对用户素材可用（内置素材为程序资产，置灰禁用）
    let can_edit = materials
        .entries
        .get(target)
        .map(|e| e.source == MaterialSource::User)
        .unwrap_or(false);

    Stack::new()
        .push(content)
        .push(super::material_context_menu::background_close_overlay())
        .push(super::material_context_menu::positioned_menu(
            target,
            can_edit,
            materials.context_menu_pos,
        ))
        .into()
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
        let item = material_item(right_sidebar, entry, idx, language);
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
///
/// 交互状态（由 `RightSidebar.materials` 驱动）：
/// - 重命名中：名称替换为输入框（回车确认）；
/// - 删除确认：由独立弹窗（`material_delete_dialog`）承接，素材行保持原交互。
fn material_item<'a>(
    right_sidebar: &'a RightSidebar,
    entry: &'a crate::right_sidebar::MaterialEntry,
    index: usize,
    language: Language,
) -> Element<'a> {
    let t = main_translations(language);
    let materials = &right_sidebar.materials;
    let is_renaming = materials.renaming_material.as_ref().map(|(i, _)| *i) == Some(index);

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

    // 重命名编辑态：名称替换为输入框（回车确认）
    let name_area: Element<'a> = if is_renaming {
        let buffer = materials
            .renaming_material
            .as_ref()
            .map(|(_, b)| b.clone())
            .unwrap_or_default();
        text_input("素材名称", &buffer)
            .on_input(|value| {
                Message::RightSidebar(RightSidebarAction::MaterialRenameInputChanged(value))
            })
            .on_submit(Message::RightSidebar(
                RightSidebarAction::MaterialRenameConfirmed,
            ))
            .padding([2, 4])
            .size(12)
            .width(Length::Fill)
            .into()
    } else {
        name_text.into()
    };

    let info_row = row![name_area]
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
            .on_right_press(Message::RightSidebar(
                RightSidebarAction::MaterialContextMenuOpened(index),
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
