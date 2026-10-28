//! 形状抽屉视图（DRAW-033B0/B1/B2）
//!
//! 左侧是形状库与文件/变换/插入按钮；右侧是 B1 Canvas 画板。

use iced_core::text::Wrapping;
use iced_core::{Background, Length, mouse};
use iced_widget::canvas::Canvas;
use iced_widget::{MouseArea, button, column, container, row, scrollable, space, text, text_input};

use crate::message::ShapeDrawerAction;
use crate::shape_drawer::canvas::ShapeDrawerCanvas;
use crate::state::root_state::ShapeDrawerState;
use crate::{Element, Message, Theme};

/// 渲染形状抽屉。
pub fn view_shape_drawer<'a>(state: &ShapeDrawerState, theme: &'a Theme) -> Element<'a> {
    let palette = theme.extended_palette();
    let panel_bg = palette.background.base.color;
    let panel_text = palette.background.base.text;
    let split_color = palette.background.strong.color;

    let rows = state
        .library
        .all()
        .map(|drawing| {
            let selected = state.selected.as_deref() == Some(drawing.id.as_str());
            button(
                text(format!(
                    "{}{}",
                    if selected { "▸ " } else { "" },
                    drawing.name
                ))
                .width(Length::Fill)
                .wrapping(Wrapping::Word)
                .color(panel_text),
            )
            .width(Length::Fill)
            .on_press(Message::ShapeDrawer(ShapeDrawerAction::Select(
                drawing.id.clone(),
            )))
            .into()
        })
        .collect::<Vec<Element<'_>>>();

    let status = state
        .status
        .as_deref()
        .map(|value| text(value.to_string()).size(11).color(panel_text))
        .unwrap_or_else(|| text("").size(11));
    let warning = state.warnings.first().map(|warning| {
        text(format!(
            "跳过 {}: {}",
            warning.path.display(),
            warning.message
        ))
        .size(10)
        .width(Length::Fill)
        .wrapping(Wrapping::Word)
        .color(panel_text)
    });
    let name_input = text_input("Shape name", &state.name_input)
        .width(Length::Fill)
        .on_input(|value| Message::ShapeDrawer(ShapeDrawerAction::SetName(value)));

    let file_actions = row![
        action_button("New", ShapeDrawerAction::New),
        action_button("Save", ShapeDrawerAction::Save),
        action_button("Rename", ShapeDrawerAction::Rename),
        action_button("Del", ShapeDrawerAction::Delete),
    ]
    .spacing(4);
    let edit_actions = row![
        action_button("Copy", ShapeDrawerAction::Copy),
        action_button("Paste", ShapeDrawerAction::Paste),
        action_button("FlipH", ShapeDrawerAction::FlipHorizontal),
        action_button("FlipV", ShapeDrawerAction::FlipVertical),
        action_button("Rot90", ShapeDrawerAction::Rotate90),
    ]
    .spacing(4);

    let panel = container(
        column![
            text("Shape Library").size(16).color(panel_text),
            scrollable(column(rows).spacing(2)).height(Length::Fill),
            name_input,
            file_actions,
            edit_actions,
            status,
            warning,
            button("Insert")
                .width(Length::Fill)
                .on_press(Message::ShapeDrawer(ShapeDrawerAction::Insert)),
        ]
        .spacing(6)
        .padding(10)
        .height(Length::Fill),
    )
    .width(Length::Fixed(state.panel_width))
    .height(Length::Fill)
    .style(move |_theme: &Theme| container::Style {
        background: Some(Background::Color(panel_bg)),
        text_color: Some(panel_text),
        ..Default::default()
    });

    let splitter = MouseArea::new(
        container(space())
            .width(Length::Fixed(6.0))
            .height(Length::Fill)
            .style(move |_theme: &Theme| container::Style {
                background: Some(Background::Color(split_color)),
                ..Default::default()
            }),
    )
    .on_press(Message::ShapeDrawer(ShapeDrawerAction::PanelDragStart))
    .on_release(Message::ShapeDrawer(ShapeDrawerAction::PanelDragEnd))
    .on_exit(Message::ShapeDrawer(ShapeDrawerAction::PanelDragEnd))
    .on_move(|point| Message::ShapeDrawer(ShapeDrawerAction::SetPanelWidth(point.x)))
    .interaction(mouse::Interaction::ResizingHorizontally);

    let canvas = Canvas::new(ShapeDrawerCanvas {
        doc: state.doc.clone(),
        interaction: state.interaction.clone(),
        shapes_dir: if state.shapes_dir.as_os_str().is_empty() {
            "not injected".to_string()
        } else {
            state.shapes_dir.display().to_string()
        },
    })
    .width(Length::Fill)
    .height(Length::Fill);

    row![panel, splitter, canvas]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn action_button<'a>(label: &'static str, action: ShapeDrawerAction) -> Element<'a> {
    button(label).on_press(Message::ShapeDrawer(action)).into()
}
