//! 云存储文件浏览面板（仿资源管理器）
//!
//! 设备切换/目录导航/文件列表/下载/保存，独立对话框与设置面板云管理页共用。

use iced_core::{Alignment, Length};
use iced_widget::{Space, button, column, container, pick_list, row, scrollable, text, text_input};

use lumino_message::CloudAction;

use crate::state::cloud_state::{CloudEntryUi, CloudUiState, format_size};
use crate::{Element, Message, Theme};

/// 云文件浏览面板（仿资源管理器）
pub fn view_cloud_browser<'a>(state: &'a CloudUiState, _theme: &Theme) -> Element<'a> {
    // 设备切换下拉
    let devices: Vec<CloudDeviceOption> = state
        .connections
        .iter()
        .map(|c| CloudDeviceOption {
            id: c.id.clone(),
            label: format!(
                "{} [{}] {}",
                c.name,
                c.protocol,
                if c.online { "在线" } else { "离线" }
            ),
        })
        .collect();
    let selected_device = state
        .selected_id
        .as_ref()
        .and_then(|id| devices.iter().find(|d| &d.id == id).cloned());
    let device_pick = pick_list(devices, selected_device, |opt| {
        Message::Cloud(CloudAction::SelectStorage(opt.id))
    })
    .placeholder("选择云存储");

    // 顶部：设备切换 + 刷新 + 断开
    let tool_row = row![
        device_pick.width(Length::Fill),
        button(text("刷新").size(12))
            .padding([4, 10])
            .on_press(Message::Cloud(CloudAction::Refresh)),
        button(text("断开").size(12))
            .padding([4, 10])
            .on_press_maybe(
                state
                    .selected_id
                    .as_ref()
                    .map(|id| Message::Cloud(CloudAction::Disconnect(id.clone()))),
            ),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    // 导航栏：返回 + 当前路径
    let nav_row = row![
        button(text("← 返回").size(12))
            .padding([4, 10])
            .on_press(Message::Cloud(CloudAction::Back)),
        text(if state.current_path.is_empty() {
            "/"
        } else {
            &state.current_path
        })
        .size(13)
        .width(Length::Fill),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    // 新建文件夹输入行
    let new_folder_row = row![
        text_input("新建文件夹名称", &state.new_folder_input)
            .on_input(|s| Message::Cloud(CloudAction::NewFolderInputChanged(s)))
            .width(Length::Fill),
        button(text("新建文件夹").size(12))
            .padding([4, 10])
            .on_press(Message::Cloud(CloudAction::NewFolder(
                state.new_folder_input.clone(),
            ))),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    // 文件列表（按入口场景过滤文件类型）
    let list: Vec<Element<'static>> = if state.busy && state.entries.is_empty() {
        vec![placeholder_text("加载中...")]
    } else if state.entries.is_empty() {
        vec![placeholder_text("（空目录）")]
    } else {
        state
            .entries
            .iter()
            .filter(|e| {
                state
                    .filter
                    .as_ref()
                    .map(|f| e.is_dir || e.name.to_lowercase().ends_with(&format!(".{f}")))
                    .unwrap_or(true)
            })
            .map(|e| entry_row(state, e))
            .collect()
    };

    let notice: Option<Element<'a>> = state.notice.as_deref().map(|n| placeholder_text(n));

    // 保存模式面板（选择上传目标目录）
    let save_panel: Element<'static> = if state.save_mode {
        container(
            column![
                text(format!(
                    "将保存当前工程到：{}",
                    if state.current_path.is_empty() {
                        "/"
                    } else {
                        &state.current_path
                    }
                ))
                .size(13),
                button(
                    text(if state.busy {
                        "上传中..."
                    } else {
                        "保存到此处"
                    })
                    .size(14)
                )
                .padding([6, 20])
                .on_press(Message::Cloud(CloudAction::SaveHere)),
            ]
            .spacing(8)
            .align_x(Alignment::Start),
        )
        .padding(10)
        .style(|theme: &Theme| container::Style {
            background: Some(iced_core::Background::Color(
                theme.extended_palette().background.weak.color,
            )),
            border: iced_core::Border::default().rounded(6),
            ..Default::default()
        })
        .into()
    } else {
        Space::new().height(0).into()
    };

    let content_col = column![tool_row, nav_row, new_folder_row]
        .spacing(8)
        .push(save_panel)
        .push(container(
            scrollable(column(list).spacing(4)).height(Length::FillPortion(1)),
        ))
        .push(notice.unwrap_or_else(|| Space::new().height(0).into()));

    container(content_col)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(16)
        .style(|theme: &Theme| container::Style {
            background: Some(iced_core::Background::Color(theme.palette().background)),
            ..Default::default()
        })
        .into()
}

/// 弱化提示文本
fn placeholder_text<'a>(content: &'a str) -> Element<'a> {
    text(content)
        .size(12)
        .style(|theme: &Theme| text::Style {
            color: Some(theme.extended_palette().background.strong.text),
        })
        .into()
}

/// 设备下拉选项
#[derive(Debug, Clone, PartialEq, Eq)]
struct CloudDeviceOption {
    id: String,
    label: String,
}

impl std::fmt::Display for CloudDeviceOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label)
    }
}

/// 单行条目：名称 + 大小 + 操作按钮
fn entry_row(state: &CloudUiState, entry: &CloudEntryUi) -> Element<'static> {
    let name_btn = if entry.is_dir {
        // 目录：点击进入
        button(
            text(format!("📁 {}", entry.name))
                .size(13)
                .width(Length::Fill),
        )
        .padding([3, 8])
        .style(|_theme: &Theme, _status| button::Style::default())
        .on_press(Message::Cloud(CloudAction::EnterDir(entry.path.clone())))
    } else {
        // 文件：不可点击（无直接打开语义），仅展示
        button(
            text(format!("📄 {}", entry.name))
                .size(13)
                .width(Length::Fill),
        )
        .padding([3, 8])
        .style(|_theme: &Theme, _status| button::Style::default())
        .on_press(Message::Null)
    };

    let size_text = text(if entry.is_dir {
        String::new()
    } else {
        format_size(entry.size)
    })
    .size(12)
    .width(Length::Fixed(80.0));

    let action_btn: Element<'static> = if !state.save_mode && !entry.is_dir {
        button(text("下载").size(12))
            .padding([2, 8])
            .on_press_maybe(
                state
                    .filter
                    .as_ref()
                    .map(|f| entry.name.to_lowercase().ends_with(&format!(".{f}")))
                    .unwrap_or(true)
                    .then(|| {
                        Message::Cloud(CloudAction::Download {
                            path: entry.path.clone(),
                        })
                    }),
            )
            .into()
    } else {
        Space::new().width(60.0).into()
    };

    row![name_btn, size_text, action_btn]
        .spacing(6)
        .align_y(Alignment::Center)
        .into()
}
