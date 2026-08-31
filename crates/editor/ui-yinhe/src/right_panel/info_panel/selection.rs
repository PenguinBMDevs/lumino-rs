//! 选框信息面板 — 对应 yinhe `right_panel/info_panel/selection.rs:724`
//!
//! 优先于 Anchor / Track / Project 显示：任一视图（PR/AR/AM）存在选框时
//! 整个 Info 面板切换为选框信息。编辑字段支持表达式：
//! 赋值（`100`）、加减（`+2`/`-2`）、乘除（`x2`/`*2`/`/2`）、百分比、链式等，
//! 语法见 `yinhe_editor_core::num_expr`，iced 桩以占位输入框保留语义。

use iced_core::Length;
use iced_widget::{button, column, container, row, text};

use lumino_ui_core::{Element, Theme, window::Window};

/// 当前拥有选框的视图（三视图互斥）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectionViewKind {
    #[default]
    Pr,
    Ar,
    Am,
}

/// 选框统计摘要（对齐 yinhe `batch_ops::summarize_selected` 精简）
#[derive(Debug, Clone, Default)]
pub struct SelectionSummary {
    pub view: SelectionViewKind,
    pub rect_count: usize,
    pub count: usize,
    pub event_count: usize,
    pub tick_span: (f64, f64),
    pub key_span: Option<(u8, u8)>,
    pub track_span: Option<(usize, usize)>,
    pub value_span: Option<(f32, f32)>,
    pub uniform_velocity: Option<u8>,
    pub uniform_gate: Option<u32>,
    pub uniform_key: Option<u8>,
    pub uniform_tick: Option<u32>,
}

/// AM 选中锚点统计（对齐 yinhe `AmAnchors`）
#[derive(Debug, Clone, Default)]
pub struct AmSelectionState {
    pub count: usize,
    pub uniform_value: Option<f32>,
    pub uniform_tick: Option<u32>,
    pub value_range: Option<(f32, f32)>,
}

fn info_row<'a>(window: &'a Window, label: &'a str, value: String) -> Element<'a> {
    let palette = window.theme.extended_palette();
    row![
        text(label).size(11).style(move |_theme: &Theme| {
            iced_widget::text::Style {
                color: Some(palette.background.weak.text),
            }
        }),
        text(value).size(12).style(move |_theme: &Theme| {
            iced_widget::text::Style {
                color: Some(palette.background.strong.text),
            }
        }),
    ]
    .spacing(6)
    .into()
}

fn field_row<'a>(window: &'a Window, label: &'a str, value: Option<String>) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let display = value.unwrap_or_else(|| "—".to_string());
    row![
        text(label).size(11).style(move |_theme: &Theme| {
            iced_widget::text::Style {
                color: Some(palette.background.weak.text),
            }
        }),
        container(text(display).size(11).style(move |_theme: &Theme| {
            iced_widget::text::Style {
                color: Some(palette.background.base.text),
            }
        }))
        .padding([2, 6])
        .style(move |_theme: &Theme| container::Style {
            background: Some(iced_core::Background::Color(palette.background.weak.color)),
            border: iced_core::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .width(Length::Fixed(90.0)),
        text("expr: 100 / +2 / x2 / 20% / x3/7")
            .size(9)
            .style(move |_theme: &Theme| {
                iced_widget::text::Style {
                    color: Some(palette.background.weak.text),
                }
            }),
    ]
    .spacing(6)
    .align_y(iced_core::Alignment::Center)
    .into()
}

fn fmt_tick(v: f64) -> String {
    format!("{v:.0}")
}

/// 任一视图存在选框？（对齐 yinhe `selection::has_any_selection`）
#[must_use]
pub fn has_any_selection(summary: &SelectionSummary, am: Option<&AmSelectionState>) -> bool {
    summary.rect_count > 0 || am.is_some_and(|a| a.count > 0)
}

/// 选框信息 `view()` — 统计 + 批量编辑 + 变速 + 翻转
pub fn view<'a>(
    window: &'a Window,
    summary: &'a SelectionSummary,
    am: Option<&'a AmSelectionState>,
) -> Element<'a> {
    let palette = window.theme.extended_palette();

    let (t0, t1) = summary.tick_span;
    let title = text("Selection")
        .size(13)
        .style(move |_theme: &Theme| iced_widget::text::Style {
            color: Some(palette.background.strong.text),
        });

    let pos_label = match summary.view {
        SelectionViewKind::Pr => "PR",
        SelectionViewKind::Ar => "AR",
        SelectionViewKind::Am => "AM",
    };

    let mut col = column![
        title,
        info_row(window, "Pos:", pos_label.to_string()),
        info_row(window, "Rects:", summary.rect_count.to_string()),
        info_row(window, "Notes:", summary.count.to_string()),
        info_row(
            window,
            "Events:",
            am.map(|a| a.count.to_string())
                .unwrap_or_else(|| summary.event_count.to_string())
        ),
        info_row(
            window,
            "Tick span:",
            format!(
                "{} → {} ({} ticks)",
                fmt_tick(t0),
                fmt_tick(t1),
                fmt_tick(t1 - t0)
            )
        ),
    ]
    .spacing(4)
    .padding([8, 8]);

    match summary.view {
        SelectionViewKind::Pr => {
            if let Some((kl, kh)) = summary.key_span {
                col = col.push(info_row(
                    window,
                    "Key span:",
                    format!("{kl} → {kh} ({} keys)", kh as i32 - kl as i32 + 1),
                ));
            }
        }
        SelectionViewKind::Ar => {
            if let Some((tl, th)) = summary.track_span {
                col = col.push(info_row(
                    window,
                    "Track span:",
                    format!("{tl} → {th} ({} tracks)", th - tl + 1),
                ));
            }
        }
        SelectionViewKind::Am => {
            if let Some(am_state) = am {
                let text = am_state
                    .value_range
                    .map(|(lo, hi)| format!("{lo:.2} → {hi:.2}"))
                    .unwrap_or_else(|| "Full range".to_string());
                col = col.push(info_row(window, "Value span:", text));
            }
        }
    }

    // 编辑区
    let edit_section: Element<'a> = match summary.view {
        SelectionViewKind::Pr | SelectionViewKind::Ar => column![
            field_row(
                window,
                "Velocity:",
                summary.uniform_velocity.map(|v| v.to_string())
            ),
            field_row(window, "Gate:", summary.uniform_gate.map(|g| g.to_string())),
            field_row(window, "Key:", summary.uniform_key.map(|k| k.to_string())),
            field_row(window, "Tick:", summary.uniform_tick.map(|t| t.to_string())),
        ]
        .spacing(4)
        .into(),
        SelectionViewKind::Am => {
            if let Some(am_state) = am {
                column![
                    field_row(
                        window,
                        "Value:",
                        am_state.uniform_value.map(|v| format!("{v:.2}"))
                    ),
                    field_row(
                        window,
                        "Tick:",
                        am_state.uniform_tick.map(|t| t.to_string())
                    ),
                ]
                .spacing(4)
                .into()
            } else {
                column![].into()
            }
        }
    };

    let tempo_section = column![
        text("Tempo (rescale span)")
            .size(11)
            .style(move |_theme: &Theme| {
                iced_widget::text::Style {
                    color: Some(palette.background.strong.text),
                }
            }),
        row![
            text("Ticks:").size(11),
            container(text(fmt_tick(t1 - t0)).size(11)).padding([2, 6]),
            text("Bar.Beat.Tick:").size(11),
            container(text("1/0").size(11)).padding([2, 6]),
            text("Ratio:").size(11),
            container(text("1").size(11)).padding([2, 6]),
        ]
        .spacing(6),
    ]
    .spacing(4);

    let flip_row: Element<'a> = if summary.view != SelectionViewKind::Am {
        row![
            button(text("Flip H").size(11)).padding([4, 8]),
            button(text("Flip V").size(11)).padding([4, 8]),
        ]
        .spacing(8)
        .into()
    } else {
        column![].into()
    };

    column![col, edit_section, tempo_section, flip_row,]
        .spacing(8)
        .into()
}

/// 解析后数值表达式操作（对齐 yinhe `num_expr::NumOp` 桩）
#[derive(Debug, Clone, PartialEq)]
pub enum NumOp {
    Set(f64),
    Add(f64),
    Mul(f64),
    Div(f64),
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_ui_core::window::Window;

    #[test]
    fn selection_view_pr() {
        let window = Window::new("Tokyo Night Storm");
        let summary = SelectionSummary {
            view: SelectionViewKind::Pr,
            rect_count: 1,
            count: 5,
            tick_span: (0.0, 1920.0),
            key_span: Some((60, 72)),
            uniform_velocity: Some(100),
            uniform_key: Some(60),
            ..Default::default()
        };
        let _el = view(&window, &summary, None);
    }

    #[test]
    fn selection_view_am() {
        let window = Window::new("Tokyo Night Storm");
        let summary = SelectionSummary {
            view: SelectionViewKind::Am,
            rect_count: 2,
            tick_span: (0.0, 960.0),
            ..Default::default()
        };
        let am = AmSelectionState {
            count: 3,
            uniform_value: Some(64.0),
            value_range: Some((0.0, 127.0)),
            ..Default::default()
        };
        let _el = view(&window, &summary, Some(&am));
    }
}
