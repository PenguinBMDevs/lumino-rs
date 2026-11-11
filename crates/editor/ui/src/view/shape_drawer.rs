//! 形状抽屉视图（DRAW-033B0/B1/B2）
//!
//! 左侧是形状库与文件/变换/插入按钮；右侧是 B1 Canvas 画板。

use iced_core::text::Wrapping;
use iced_core::{Background, Length, mouse};
use iced_widget::canvas::Canvas;
use iced_widget::{
    MouseArea, Stack, button, column, container, row, scrollable, space, text, text_input,
};
use lumino_core::shape::{AnchorBend, ShapeFill, SpamAlign, SpamEnds, StrokeKind};

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

    // 面板可缩到 180px，固定 row 会把后半排按钮裁掉；交给 iced 的横向
    // wrapping 布局，放不下时自动换到下一行。
    let file_actions = row![
        action_button("New", ShapeDrawerAction::New),
        action_button("Save", ShapeDrawerAction::Save),
        action_button("Rename", ShapeDrawerAction::Rename),
        action_button("Del", ShapeDrawerAction::Delete),
    ]
    .spacing(4)
    .wrap();
    let edit_actions = row![
        action_button("Copy", ShapeDrawerAction::Copy),
        action_button("Paste", ShapeDrawerAction::Paste),
        action_button("FlipH", ShapeDrawerAction::FlipHorizontal),
        action_button("FlipV", ShapeDrawerAction::FlipVertical),
        action_button("Rot90", ShapeDrawerAction::Rotate90),
    ]
    .spacing(4)
    .wrap();

    let area_mode = state.doc.borrow().area_mode;
    let area_colour = state.doc.borrow().area_colour;
    let max_colour = state.doc.borrow().max_colour;
    let area_toggle = button(if area_mode { "Area: on" } else { "Area" })
        .width(Length::Fill)
        .on_press(Message::ShapeDrawer(ShapeDrawerAction::ToggleAreaMode));
    let area_hint = if state.doc.borrow().can_area_paint() {
        format!("current colour: {area_colour}")
    } else {
        "single closed stroke only".to_string()
    };
    // DRAW-032：16 号色常驻显示（= GM 通道 10）。未启用工程开关时点击会被拒绝并给出提示。
    let swatches = (0..=16u8)
        .map(|colour| {
            button(text(format!("{colour}")))
                .on_press(Message::ShapeDrawer(ShapeDrawerAction::SetAreaColour(
                    colour,
                )))
                .into()
        })
        .collect::<Vec<Element<'_>>>();
    let use10_hint = if max_colour >= 16 {
        "16 = GM 通道 10"
    } else {
        "16 = 通道 10（需在工程设置启用）"
    };

    // DRAW-011：填充档位与三开关
    let (fill, align, ends, union, apart, borders, gate_beats) = {
        let doc = state.doc.borrow();
        (
            doc.fill,
            doc.align,
            doc.ends,
            doc.union,
            doc.apart,
            doc.borders,
            doc.gate_beats,
        )
    };
    let fill_label = match fill {
        ShapeFill::Empty => "Fill: Empty",
        ShapeFill::Fill => "Fill: Fill",
        ShapeFill::Spam => "Fill: Spam",
        ShapeFill::OutlineSpam => "Fill: Outline",
    };
    let align_label = match align {
        SpamAlign::Auto => "Align: Auto",
        SpamAlign::Aligned => "Align: Aligned",
        SpamAlign::Centred => "Align: Centred",
    };
    let ends_label = match ends {
        SpamEnds::Round => "Ends: Round",
        SpamEnds::Keep => "Ends: Keep",
        SpamEnds::Drop => "Ends: Drop",
        SpamEnds::Min => "Ends: Min",
        SpamEnds::Stretch => "Ends: Stretch",
    };
    let fill_row = row![
        action_button(fill_label, ShapeDrawerAction::CycleFill),
        action_button(align_label, ShapeDrawerAction::CycleAlign),
        action_button(ends_label, ShapeDrawerAction::CycleEnds),
    ]
    .spacing(4)
    .wrap();
    let gate_step = 0.0625f32;
    let gate_row = row![
        text("Gate").size(11),
        button("-").on_press(Message::ShapeDrawer(ShapeDrawerAction::SetGateBeats(
            (gate_beats - gate_step).max(0.001)
        ))),
        text(format!("{gate_beats:.3}")).size(11),
        button("+").on_press(Message::ShapeDrawer(ShapeDrawerAction::SetGateBeats(
            gate_beats + gate_step
        ))),
    ]
    .spacing(4)
    .align_y(iced_core::Alignment::Center);
    let flag = |name: &'static str, on: bool, action: ShapeDrawerAction| -> Element<'_> {
        button(text(format!("{name}: {}", if on { "On" } else { "Off" })))
            .on_press(Message::ShapeDrawer(action))
            .into()
    };
    let switch_row = row![
        flag("Union", union, ShapeDrawerAction::ToggleUnion),
        flag("Apart", apart, ShapeDrawerAction::ToggleApart),
        flag("Borders", borders, ShapeDrawerAction::ToggleBorders),
    ]
    .spacing(4)
    .wrap();
    let area_actions = row![
        area_toggle,
        button("Clear").on_press(Message::ShapeDrawer(ShapeDrawerAction::ClearAreas)),
    ]
    .spacing(4);
    let area_palette = row(swatches).spacing(2).wrap();

    let insert_size = row![
        text("Size").size(11),
        text_input("beats", &state.insert_beats_text)
            .width(Length::Fixed(42.0))
            .on_input(|value| Message::ShapeDrawer(ShapeDrawerAction::SetInsertBeats(value))),
        text("x").size(11),
        text_input("keys", &state.insert_keys_text)
            .width(Length::Fixed(42.0))
            .on_input(|value| Message::ShapeDrawer(ShapeDrawerAction::SetInsertKeys(value))),
        button(if state.insert_fit_contain {
            "Contain"
        } else {
            "Stretch"
        })
        .on_press(Message::ShapeDrawer(ShapeDrawerAction::ToggleInsertFit)),
    ]
    .spacing(4);
    let scale_row = row![
        text("Scale").size(11),
        text_input("%", &state.scale_percent_text)
            .width(Length::Fixed(46.0))
            .on_input(|value| Message::ShapeDrawer(ShapeDrawerAction::SetScalePercent(value))),
        button("Apply").on_press(Message::ShapeDrawer(ShapeDrawerAction::ApplyScale)),
    ]
    .spacing(4);

    let anchor_bend = {
        let doc = state.doc.borrow();
        doc.selected_anchor.and_then(|(stroke, anchor)| {
            let stroke = doc.strokes.get(stroke)?;
            if stroke.kind != StrokeKind::Curve {
                return None;
            }
            stroke.anchors.get(anchor).map(|anchor| anchor.bend)
        })
    };
    let bend_control: Element = match anchor_bend {
        Some(bend) => button(if bend == AnchorBend::Corner {
            "Bend: Corner"
        } else {
            "Bend: Smooth"
        })
        .on_press(Message::ShapeDrawer(ShapeDrawerAction::ToggleAnchorBend))
        .into(),
        None => space().into(),
    };

    // DRAW-011：控件区尽量压在一屏内（面板最小高度 670px）
    let controls = column![
        name_input,
        file_actions,
        edit_actions,
        area_actions,
        area_palette,
        text(format!("{use10_hint} · {area_hint}"))
            .size(10)
            .color(panel_text),
        fill_row,
        gate_row,
        switch_row,
        status,
        warning,
        insert_size,
        scale_row,
        bend_control,
        button("Insert")
            .width(Length::Fill)
            .on_press(Message::ShapeDrawer(ShapeDrawerAction::Insert)),
    ]
    .spacing(3)
    .width(Length::Fill);

    let panel = container(
        column![
            text("Shape Library").size(16).color(panel_text),
            scrollable(column(rows).spacing(2)).height(Length::Fill),
            controls,
        ]
        .spacing(3)
        .padding(6)
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

    let content: Element<'a> = row![panel, splitter, canvas]
        .width(Length::Fill)
        .height(Length::Fill)
        .into();

    if state.panel_dragging {
        // 分割线只有 6px 宽；拖动时用全窗透明层接住 CursorMoved/Released，
        // 否则鼠标一离开分割线，on_move 就断流，表现为“跳一下就拖不动”。
        let drag_overlay =
            MouseArea::new(container(space()).width(Length::Fill).height(Length::Fill))
                .on_move(|point| Message::ShapeDrawer(ShapeDrawerAction::SetPanelWidth(point.x)))
                .on_release(Message::ShapeDrawer(ShapeDrawerAction::PanelDragEnd))
                .interaction(mouse::Interaction::ResizingHorizontally);
        return Stack::new()
            .push(content)
            .push(drag_overlay)
            .width(Length::Fill)
            .height(Length::Fill)
            .into();
    }

    content
}

fn action_button<'a>(label: &'static str, action: ShapeDrawerAction) -> Element<'a> {
    button(label).on_press(Message::ShapeDrawer(action)).into()
}
