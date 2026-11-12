//! 撤销历史面板视图（DRAW-035）
//!
//! 只读时间线：最老在上，当前游标高亮，未来步骤浅色；点击任何一行把
//! `HistoryAction::Jump(index)` 交给独立窗口 Root，再由 Runner 调主编辑器
//! 执行（保证形状 group 与协作广播一起回放）。

use iced_core::{Border, Length};
use iced_widget::{button, column, container, row, scrollable, space, text};

use lumino_core::tr;
use lumino_ui_core::resources::icon::{self, Icon};

use crate::message::{HistoryAction, Message};
use crate::state::root_state::{HistoryDialogState, HistoryKind};

/// 列表行高
const ROW_HEIGHT: f32 = 32.0;
/// 当前步骤标记的图标边长（逻辑像素）
const MARKER_SIZE: u32 = 16;

/// 渲染撤销历史面板
pub fn view_history_dialog<'a>(
    state: &'a HistoryDialogState,
    theme: &'a iced_core::Theme,
) -> crate::Element<'a> {
    let palette = theme.extended_palette();
    let text_color = palette.background.base.text;
    let hint_color = palette.background.strong.text;

    let title = text(tr!("撤销历史"))
        .size(18)
        .style(move |_theme: &iced_core::Theme| text::Style {
            color: Some(text_color),
        });
    let hint = text(tr!("点击步骤回到该步完成后的状态；初始状态为 0。"))
        .size(12)
        .style(move |_theme: &iced_core::Theme| text::Style {
            color: Some(hint_color),
        });

    let colors = RowPalette {
        current_bg: palette.primary.weak.color,
        text: palette.background.base.text,
        future: palette.background.strong.text,
    };

    let mut rows: Vec<crate::Element<'a>> = Vec::with_capacity(state.steps.len() + 1);
    rows.push(history_row(
        0,
        tr!("初始状态"),
        state.current == 0,
        state.current > 0,
        colors,
        theme,
    ));

    for step in &state.steps {
        let is_current = step.index == state.current;
        let is_future = step.index > state.current;
        rows.push(history_row(
            step.index,
            history_label(step.kind),
            is_current,
            is_future,
            colors,
            theme,
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
    // DEBT-13：去掉内层圆角 + 边框。弹窗本身已有外框，这里再画 8px 圆角会
    // 形成「窗中窗」；设置 / 保存确认等弹窗都只铺背景色，本窗口与之对齐。
    .style(move |_theme: &iced_core::Theme| container::Style {
        background: Some(iced_core::Background::Color(palette.background.base.color)),
        ..Default::default()
    });

    panel.into()
}

/// 时间线行配色（同一窗口内恒定；打包传递，避免函数参数膨胀）
#[derive(Clone, Copy)]
struct RowPalette {
    /// 当前步骤行底色
    current_bg: iced_core::Color,
    /// 已发生步骤的文字色
    text: iced_core::Color,
    /// 未来步骤（可重做）的文字色
    future: iced_core::Color,
}

fn history_row<'a>(
    index: usize,
    label: &'a str,
    is_current: bool,
    is_future: bool,
    colors: RowPalette,
    theme: &'a iced_core::Theme,
) -> crate::Element<'a> {
    let color = if is_future {
        colors.future
    } else {
        colors.text
    };
    // DEBT-13：当前步骤标记改用项目图标体系（history-undo），不再用 "▶ "
    // （U+25B6 在部分字体/平台会按 Emoji 呈现，且与既有图标体系不一致）。
    // 非当前行放等宽 Space 占位，保证所有步骤名左边缘对齐、切换步骤时不跳动。
    let marker: crate::Element<'a> = if is_current {
        icon::view_with_size_and_theme(Icon::Undo, MARKER_SIZE, MARKER_SIZE, Some(theme))
    } else {
        space()
            .width(Length::Fixed(MARKER_SIZE as f32))
            .height(Length::Fixed(MARKER_SIZE as f32))
            .into()
    };

    let content = row![
        marker,
        text(label)
            .size(13)
            .style(move |_theme: &iced_core::Theme| text::Style { color: Some(color) }),
    ]
    .spacing(6)
    .align_y(iced_core::Alignment::Center);

    button(content)
        .width(Length::Fill)
        .height(Length::Fixed(ROW_HEIGHT))
        .padding([4, 8])
        .style(move |_theme: &iced_core::Theme, status| {
            let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
            iced_widget::button::Style {
                background: Some(iced_core::Background::Color(if is_current {
                    colors.current_bg
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
        HistoryKind::ShapeEdit => tr!("形状编辑"),
        HistoryKind::Other => tr!("其他操作"),
    }
}
