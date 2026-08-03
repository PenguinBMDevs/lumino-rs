//! 找回删除音轨对话框视图
//!
//! 列表展示当前缓存目录下的 `.lmdeltrack` 文件（文件名、删除时间、音符总数），
//! 底部提供"永久删除"与"恢复"两个操作按钮。
//!
//! 实际磁盘 I/O 由 Runner 完成，本视图仅触发 `RecoverTrackAction`，
//! 由 Root 转换为 `DialogResult::RecoverTrack*` 后交给 Runner 执行。

use iced_core::{Border, Length};
use iced_widget::{button, column, container, row, scrollable, space, text};

use crate::message::{Message, RecoverTrackAction};
use crate::state::root_state::{RecoverTrackDialogState, RecoverTrackEntry};

/// 列表行高
const ROW_HEIGHT: f32 = 36.0;
/// 列表区域最大高度（超出滚动）
const LIST_MAX_HEIGHT: f32 = 400.0;
/// 表头列宽比例（文件名 : 删除时间 : 音符总数）
const COL_FILENAME: f32 = 240.0;
const COL_DELETED_AT: f32 = 180.0;
#[allow(dead_code)]
const COL_NOTE_COUNT: f32 = 80.0;

/// 渲染找回删除音轨对话框
pub fn view_recover_track_dialog<'a>(
    state: &'a RecoverTrackDialogState,
    theme: &'a iced_core::Theme,
) -> crate::Element<'a> {
    let palette = theme.extended_palette();
    let text_color = palette.background.neutral.text;
    let hint_color = palette.background.strong.text;
    let header_bg = palette.background.weak.color;
    let row_hover_bg = palette.background.weak.color;
    let row_selected_bg = palette.primary.base.color;
    let row_selected_text = palette.primary.base.text;
    let border_color = palette.background.strong.color;

    let title = text("找回删除音轨")
        .size(18)
        .style(move |_theme: &iced_core::Theme| text::Style {
            color: Some(text_color),
        });

    let hint =
        text("选中条目后点击「恢复」可还原至原位置；「永久删除」将销毁缓存文件并释放轨道编号。")
            .size(12)
            .style(move |_theme: &iced_core::Theme| text::Style {
                color: Some(hint_color),
            });

    // 表头
    let header = row![
        container(
            text("文件名")
                .size(13)
                .style(move |_theme: &iced_core::Theme| text::Style {
                    color: Some(text_color),
                })
        )
        .width(Length::Fixed(COL_FILENAME))
        .align_x(iced_core::alignment::Horizontal::Left)
        .padding([6, 8]),
        container(
            text("删除时间")
                .size(13)
                .style(move |_theme: &iced_core::Theme| text::Style {
                    color: Some(text_color),
                })
        )
        .width(Length::Fixed(COL_DELETED_AT))
        .align_x(iced_core::alignment::Horizontal::Left)
        .padding([6, 8]),
        container(
            text("音符总数")
                .size(13)
                .style(move |_theme: &iced_core::Theme| text::Style {
                    color: Some(text_color),
                })
        )
        .width(Length::Fill)
        .align_x(iced_core::alignment::Horizontal::Right)
        .padding([6, 8]),
    ]
    .align_y(iced_core::Alignment::Center)
    .height(Length::Fixed(ROW_HEIGHT));

    let header_container = container(header)
        .width(Length::Fill)
        .height(Length::Fixed(ROW_HEIGHT))
        .style(move |_theme: &iced_core::Theme| container::Style {
            background: Some(header_bg.into()),
            border: Border {
                radius: 4.0.into(),
                width: 1.0,
                color: border_color,
            },
            ..Default::default()
        });

    // 列表行
    let mut list = column![].spacing(2);
    if state.entries.is_empty() {
        list = list.push(
            container(text("暂无已删除的音轨缓存").size(13).style(
                move |_theme: &iced_core::Theme| text::Style {
                    color: Some(hint_color),
                },
            ))
            .width(Length::Fill)
            .height(Length::Fixed(80.0))
            .align_x(iced_core::alignment::Horizontal::Center)
            .align_y(iced_core::alignment::Vertical::Center),
        );
    } else {
        for (idx, entry) in state.entries.iter().enumerate() {
            let is_selected = state.selected_index == Some(idx);
            let row_bg = if is_selected {
                row_selected_bg
            } else {
                iced_core::Color::TRANSPARENT
            };
            let row_fg = if is_selected {
                row_selected_text
            } else {
                text_color
            };

            let filename_text =
                text(&entry.filename)
                    .size(13)
                    .style(move |_theme: &iced_core::Theme| text::Style {
                        color: Some(row_fg),
                    });
            let deleted_at_text =
                text(&entry.deleted_at)
                    .size(13)
                    .style(move |_theme: &iced_core::Theme| text::Style {
                        color: Some(row_fg),
                    });
            let note_count_text = text(format!("{}", entry.note_count)).size(13).style(
                move |_theme: &iced_core::Theme| text::Style {
                    color: Some(row_fg),
                },
            );

            let row_content = row![
                container(filename_text)
                    .width(Length::Fixed(COL_FILENAME))
                    .align_x(iced_core::alignment::Horizontal::Left)
                    .padding([6, 8]),
                container(deleted_at_text)
                    .width(Length::Fixed(COL_DELETED_AT))
                    .align_x(iced_core::alignment::Horizontal::Left)
                    .padding([6, 8]),
                container(note_count_text)
                    .width(Length::Fill)
                    .align_x(iced_core::alignment::Horizontal::Right)
                    .padding([6, 8]),
            ]
            .align_y(iced_core::Alignment::Center)
            .height(Length::Fixed(ROW_HEIGHT));

            // 选中行用按钮捕获点击，触发 SelectionChanged
            let row_btn = button(row_content)
                .width(Length::Fill)
                .on_press(Message::RecoverTrack(RecoverTrackAction::SelectionChanged(
                    idx,
                )))
                .style(move |_theme: &iced_core::Theme, status| {
                    let bg = match status {
                        button::Status::Hovered if !is_selected => row_hover_bg,
                        _ => row_bg,
                    };
                    button::Style {
                        background: Some(bg.into()),
                        text_color: row_fg,
                        border: Border {
                            radius: 4.0.into(),
                            width: 0.0,
                            color: iced_core::Color::TRANSPARENT,
                        },
                        snap: false,
                        shadow: Default::default(),
                    }
                });

            list = list.push(row_btn);
        }
    }

    let scrollable_list = scrollable(list)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new().width(8).scroller_width(6),
        ))
        .height(Length::Fill)
        .width(Length::Fill);

    // 底部按钮区
    let can_act = state
        .selected_index
        .and_then(|i| state.entries.get(i))
        .is_some();

    let selected_entry: Option<&RecoverTrackEntry> =
        state.selected_index.and_then(|i| state.entries.get(i));

    let restore_button = {
        let entry = selected_entry;
        let enabled = can_act && entry.is_some();
        let mut btn = button(
            text("恢复")
                .size(14)
                .style(move |_theme: &iced_core::Theme| text::Style {
                    color: Some(if enabled {
                        iced_core::Color::WHITE
                    } else {
                        palette.background.weak.text
                    }),
                }),
        )
        .padding([8, 24])
        .width(Length::Fixed(110.0));
        if let Some(e) = entry {
            btn = btn.on_press(Message::RecoverTrack(RecoverTrackAction::Restore {
                path: e.path.clone(),
                original_index: e.original_index,
            }));
        }
        btn.style(move |_theme: &iced_core::Theme, status| {
            let bg = match status {
                button::Status::Hovered if enabled => palette.primary.strong.color,
                _ if enabled => palette.primary.base.color,
                _ => palette.background.weak.color,
            };
            button::Style {
                background: Some(bg.into()),
                text_color: if enabled {
                    iced_core::Color::WHITE
                } else {
                    palette.background.weak.text
                },
                border: Border {
                    radius: 4.0.into(),
                    width: 0.0,
                    color: iced_core::Color::TRANSPARENT,
                },
                snap: false,
                shadow: Default::default(),
            }
        })
    };

    let permanently_delete_button =
        {
            let entry = selected_entry;
            let enabled = can_act && entry.is_some();
            let mut btn = button(text("永久删除").size(14).style(
                move |_theme: &iced_core::Theme| text::Style {
                    color: Some(if enabled {
                        iced_core::Color::WHITE
                    } else {
                        palette.background.weak.text
                    }),
                },
            ))
            .padding([8, 24])
            .width(Length::Fixed(110.0));
            if let Some(e) = entry {
                btn = btn.on_press(Message::RecoverTrack(
                    RecoverTrackAction::PermanentlyDelete {
                        path: e.path.clone(),
                        track_id: e.track_id,
                    },
                ));
            }
            btn.style(move |_theme: &iced_core::Theme, status| {
                let bg = match status {
                    button::Status::Hovered if enabled => palette.danger.strong.color,
                    _ if enabled => palette.danger.base.color,
                    _ => palette.background.weak.color,
                };
                button::Style {
                    background: Some(bg.into()),
                    text_color: if enabled {
                        iced_core::Color::WHITE
                    } else {
                        palette.background.weak.text
                    },
                    border: Border {
                        radius: 4.0.into(),
                        width: 0.0,
                        color: iced_core::Color::TRANSPARENT,
                    },
                    snap: false,
                    shadow: Default::default(),
                }
            })
        };

    let close_button = button(
        text("关闭")
            .size(14)
            .style(move |_theme: &iced_core::Theme| text::Style {
                color: Some(text_color),
            }),
    )
    .on_press(Message::RecoverTrack(RecoverTrackAction::CloseDialog))
    .padding([8, 24])
    .width(Length::Fixed(110.0))
    .style(move |_theme: &iced_core::Theme, status| {
        let bg = match status {
            button::Status::Hovered => palette.background.strong.color,
            _ => palette.background.weak.color,
        };
        button::Style {
            background: Some(bg.into()),
            text_color,
            border: Border {
                radius: 4.0.into(),
                width: 0.0,
                color: iced_core::Color::TRANSPARENT,
            },
            snap: false,
            shadow: Default::default(),
        }
    });

    let buttons_row = row![
        space().width(Length::Fill),
        permanently_delete_button,
        space().width(8),
        restore_button,
        space().width(8),
        close_button,
    ]
    .align_y(iced_core::Alignment::Center);

    // 列表区域：固定高度上限 + 滚动
    let list_section = container(scrollable_list)
        .width(Length::Fill)
        .height(Length::Fixed(LIST_MAX_HEIGHT))
        .style(move |_theme: &iced_core::Theme| container::Style {
            background: Some(palette.background.base.color.into()),
            border: Border {
                radius: 4.0.into(),
                width: 1.0,
                color: border_color,
            },
            ..Default::default()
        });

    let content = column![
        title,
        space().height(4),
        hint,
        space().height(12),
        header_container,
        space().height(4),
        list_section,
        space().height(16),
        buttons_row,
    ]
    .spacing(4);

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(24)
        .style(move |_theme: &iced_core::Theme| container::Style {
            background: Some(palette.background.base.color.into()),
            ..Default::default()
        })
        .into()
}
