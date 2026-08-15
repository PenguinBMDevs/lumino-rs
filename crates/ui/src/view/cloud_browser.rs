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

    // 顶部：设备切换 + 刷新 + 断开 + 粘贴（剪贴板非空时）
    let clipboard = state.clipboard.as_ref();
    let paste_btn = button(
        text(if clipboard.map(|c| c.is_cut).unwrap_or(false) {
            "粘贴（剪切）"
        } else {
            "粘贴"
        })
        .size(12),
    )
    .padding([4, 10])
    .on_press_maybe(clipboard.map(|_| Message::Cloud(CloudAction::Paste)));

    let clear_clip_btn = button(text("清空剪贴板").size(12))
        .padding([4, 10])
        .on_press_maybe(clipboard.map(|_| Message::Cloud(CloudAction::ClearClipboard)));

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
        paste_btn,
        clear_clip_btn,
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
        // 素材上传待办存在 → 上传素材；否则保存当前工程
        let title = match &state.pending_upload {
            Some(p) => format!(
                "将上传素材 {} 到：{}",
                p.file_name,
                if state.current_path.is_empty() {
                    "/"
                } else {
                    &state.current_path
                }
            ),
            None => format!(
                "将保存当前工程到：{}",
                if state.current_path.is_empty() {
                    "/"
                } else {
                    &state.current_path
                }
            ),
        };
        container(
            column![
                text(title).size(13),
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
            // 容器级文本色：所有子 text 未显式设置时继承，跟随主题保证可读
            text_color: Some(theme.palette().text),
            ..Default::default()
        })
        .into()
}

/// 弱化提示文本（跟随主题）
fn placeholder_text<'a>(content: &'a str) -> Element<'a> {
    text(content)
        .size(12)
        .style(|theme: &Theme| text::Style {
            color: Some(theme.extended_palette().background.base.text),
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

/// 单行条目：名称 + 大小 + 操作按钮（复制/剪切/重命名/删除）
fn entry_row(state: &CloudUiState, entry: &CloudEntryUi) -> Element<'static> {
    // 重命名编辑态：名称替换为输入框
    let is_renaming = state.renaming.as_deref() == Some(entry.path.as_str());
    let name_area: Element<'static> = if is_renaming {
        row![
            text_input("新名称", &state.rename_input)
                .on_input(|s| Message::Cloud(CloudAction::RenameInputChanged(s)))
                .width(Length::Fill),
            small_btn("确定", Message::Cloud(CloudAction::RenameConfirm)),
            small_btn("取消", Message::Cloud(CloudAction::RenameCancel)),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .into()
    } else {
        let name_btn = button(
            text(format!(
                "{} {}",
                if entry.is_dir { "📁" } else { "📄" },
                entry.name
            ))
            .size(13)
            .width(Length::Fill),
        )
        .padding([3, 8])
        // 修复：button::Style::default() 的 text_color 固定黑色，
        // 暗色主题下文字不可见——显式跟随主题文字色
        .style(|theme: &Theme, _status| button::Style {
            text_color: theme.palette().text,
            ..Default::default()
        })
        .on_press(if entry.is_dir {
            Message::Cloud(CloudAction::EnterDir(entry.path.clone()))
        } else {
            Message::Null
        });
        name_btn.into()
    };

    let size_text = text(if entry.is_dir {
        String::new()
    } else {
        format_size(entry.size)
    })
    .size(12)
    .width(Length::Fixed(80.0));

    // 删除确认态：操作区替换为确认提示
    let pending_delete = state
        .pending_delete
        .as_ref()
        .map(|(path, _, _)| path.as_str() == entry.path.as_str())
        .unwrap_or(false);

    let action_area: Element<'static> = if pending_delete {
        row![
            text("确认删除？").size(12),
            small_btn(
                "删除",
                Message::Cloud(CloudAction::DeleteEntry {
                    path: entry.path.clone(),
                    is_dir: entry.is_dir,
                })
            ),
            small_btn("取消", Message::Cloud(CloudAction::DeleteCancel)),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .into()
    } else {
        let mut actions = Vec::<Element<'static>>::new();
        if !state.save_mode && !entry.is_dir {
            // 文件行：下载
            actions.push(
                small_btn(
                    "下载",
                    Message::Cloud(CloudAction::Download {
                        path: entry.path.clone(),
                    }),
                )
                .into(),
            );
        }
        if !state.save_mode {
            if entry.is_dir {
                // 目录：复制不支持（提示），支持剪切
                actions.push(
                    small_btn(
                        "剪切",
                        Message::Cloud(CloudAction::CutEntry {
                            path: entry.path.clone(),
                            is_dir: true,
                        }),
                    )
                    .into(),
                );
            } else {
                actions.push(
                    small_btn(
                        "复制",
                        Message::Cloud(CloudAction::CopyEntry {
                            path: entry.path.clone(),
                            is_dir: false,
                        }),
                    )
                    .into(),
                );
                actions.push(
                    small_btn(
                        "剪切",
                        Message::Cloud(CloudAction::CutEntry {
                            path: entry.path.clone(),
                            is_dir: false,
                        }),
                    )
                    .into(),
                );
            }
            actions.push(
                small_btn(
                    "重命名",
                    Message::Cloud(CloudAction::StartRename(entry.path.clone())),
                )
                .into(),
            );
            actions.push(
                small_btn(
                    "删除",
                    Message::Cloud(CloudAction::RequestDelete {
                        path: entry.path.clone(),
                        is_dir: entry.is_dir,
                    }),
                )
                .into(),
            );
        }
        if actions.is_empty() {
            Space::new().width(60.0).into()
        } else {
            row(actions).spacing(6).align_y(Alignment::Center).into()
        }
    };

    row![name_area, size_text, action_area]
        .spacing(6)
        .align_y(Alignment::Center)
        .into()
}

/// 小尺寸操作按钮（跟随主题文字色）
fn small_btn<'a>(label: &'a str, msg: Message) -> Element<'a> {
    button(text(label).size(12))
        .padding([2, 8])
        .style(|theme: &Theme, _status| button::Style {
            // 修复：默认 text_color 为固定黑色，暗色主题不可见
            text_color: theme.palette().text,
            ..Default::default()
        })
        .on_press(msg)
        .into()
}
