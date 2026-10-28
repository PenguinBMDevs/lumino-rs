//! 形状抽屉空窗（DRAW-033B0）
//!
//! 本阶段只验证独立窗口生命周期与 `shapes_dir` 注入；画板、工具和库列表
//! 分别由 033B1 / 033B2 接入，避免 B0 提前锁死 UI 结构。

use iced_core::{Alignment, Length};
use iced_widget::{column, container, space, text};

use lumino_core::tr;

use crate::state::root_state::ShapeDrawerState;
use crate::{Element, Theme};

/// 渲染形状抽屉空窗占位。
pub fn view_shape_drawer<'a>(state: &ShapeDrawerState, theme: &'a Theme) -> Element<'a> {
    let path = if state.shapes_dir.as_os_str().is_empty() {
        tr!("尚未注入形状库目录").to_string()
    } else {
        state.shapes_dir.display().to_string()
    };
    let palette = theme.extended_palette();

    container(
        column![
            text(tr!("形状抽屉")).size(24),
            space().height(12),
            text(tr!("形状库目录")).size(14),
            text(path).size(13),
            space().height(24),
            text(tr!("画板与工具将在 033B1 接入")).size(14),
        ]
        .spacing(6)
        .align_x(Alignment::Start),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding(32)
    .style(move |_theme: &Theme| container::Style {
        background: Some(iced_core::Background::Color(palette.background.base.color)),
        text_color: Some(palette.background.base.text),
        ..Default::default()
    })
    .into()
}
