//! 撤销历史面板视图（DRAW-035）
//!
//! 只读时间线：最老在上，当前游标高亮，未来步骤浅色；点击任何一行把
//! `HistoryAction::Jump(index)` 交给独立窗口 Root，再由 Runner 调主编辑器
//! 执行（保证形状 group 与协作广播一起回放）。

use iced_core::{Border, Length};
use iced_widget::{button, column, container, row, scrollable, space, text};

use lumino_core::tr;

use crate::message::{HistoryAction, Message};
use crate::state::root_state::{HistoryDialogState, HistoryKind};

/// 列表行高
const ROW_HEIGHT: f32 = 32.0;

/// 渲染撤销历史面板
pub fn view_history_dialog<'a>(
    state: &'a HistoryDialogState,
    theme: &'a iced_core::Theme,
) -> crate::Element<'a> {
    let palette = theme.extended_palette();
    let text_color = palette.background.base.text;
    let hint_color = palette.background.strong.text;
    let current_bg = palette.primary.weak.color;
    let future_color = palette.background.strong.text;
    let border_color = palette.background.strong.color;

    let title = text(tr!("撤销历史"))
        .size(18)
        .style(move |_theme: &iced_core::Theme| text::Style {
            color: Some(text_color),
        });
    let hint = text(tr!("点击步骤跳转；初始状态为 0。")).size(12).style(
        move |_theme: &iced_core::Theme| text::Style {
            color: Some(hint_color),
        },
    );

    let mut rows: Vec<crate::Element<'a>> = Vec::with_capacity(state.steps.len() + 1);
    rows.push(history_row(
        0,
        tr!("初始状态"),
        state.current == 0,
        state.current > 0,
        current_bg,
        text_color,
        future_color,
    ));

    for step in &state.steps {
        let is_current = step.index == state.current;
        let is_future = step.index > state.current;
        rows.push(history_row(
            step.index,
            history_label(step.kind),
            is_current,
            is_future,
            current_bg,
            text_color,
            future_color,
        ));
    }

    let list = scrollable(column(rows).spacing(2))
        .height(Length::Fill)
        .width(Length::Fill);

    let footer = row![
        text(format!("{} / {}", state.current, state.steps.len()))
            .size(12)
            .style(move |_theme: &iced_core::Theme| text::Style {
                color: Some(hint_color),
            }),
        space().width(Length::Fill),
        button(text(tr!("关闭")).size(13))
            .padding([6, 18])
            .on_press(Message::History(HistoryAction::Close)),
    ]
    .align_y(iced_core::Alignment::Center);

    let panel = container(
        column![
            title,
            space().height(4),
            hint,
            space().height(8),
            list,
            footer
        ]
        .spacing(6),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding(16)
    .style(move |_theme: &iced_core::Theme| container::Style {
        background: Some(iced_core::Background::Color(palette.background.base.color)),
        border: Border::default().rounded(8).color(border_color).width(1),
        ..Default::default()
    });

    panel.into()
}

fn history_row<'a>(
    index: usize,
    label: &'a str,
    is_current: bool,
    is_future: bool,
    current_bg: iced_core::Color,
    text_color: iced_core::Color,
    future_color: iced_core::Color,
) -> crate::Element<'a> {
    let color = if is_future { future_color } else { text_color };
    let marker = if is_current { "▶ " } else { "  " };
    let label = format!("{marker}{label}");
    button(
        text(label)
            .size(13)
            .style(move |_theme: &iced_core::Theme| text::Style { color: Some(color) }),
    )
    .width(Length::Fill)
    .height(Length::Fixed(ROW_HEIGHT))
    .padding([4, 8])
    .style(move |_theme: &iced_core::Theme, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        iced_widget::button::Style {
            background: Some(iced_core::Background::Color(if is_current {
                current_bg
            } else if hovered {
                iced_core::Color::from_rgba(1.0, 1.0, 1.0, 0.06)
            } else {
                iced_core::Color::TRANSPARENT
            })),
            text_color: color,
            border: Border::default().rounded(4),
            ..Default::default()
        }
    })
    .on_press(Message::History(HistoryAction::Jump(index)))
    .into()
}

fn history_label(kind: HistoryKind) -> &'static str {
    match kind {
        HistoryKind::NoteCreate => tr!("新建音符"),
        HistoryKind::NoteMove => tr!("移动音符"),
        HistoryKind::NoteDelete => tr!("删除音符"),
        HistoryKind::NoteTransform => tr!("变换音符"),
        HistoryKind::VelocityEdit => tr!("调整力度"),
        HistoryKind::AutomationEdit => tr!("自动化编辑"),
        HistoryKind::Recording => tr!("MIDI 录制"),
        HistoryKind::Other => tr!("其他操作"),
    }
}
