//! 设置页面 - 调色板选择
//!
//! 用户可在已嵌入的调色板之间切换，实时预览各调色板的颜色。

use crate::{Element, Message, Theme};
use iced_core::{Alignment, Border, Length};
use iced_widget::{column, container, pick_list, row, scrollable, text};

use super::super::components::constants::*;
use super::super::components::styles::{create_content_text_style, create_placeholder_text_style};
use crate::settings::{Event, SettingsPanel};
use lumino_core::palette::PaletteColor;

/// 调色板预览颜色条的大小
const SWATCH_SIZE: f32 = 28.0;
/// 每行显示的色块数量
const SWATCHES_PER_ROW: usize = 16;
/// 色块之间的间距
const SWATCH_SPACING: f32 = 2.0;

/// 渲染调色板设置页面
pub fn view<'a>(settings: &'a SettingsPanel) -> Element<'a> {
    let t = lumino_core::i18n::settings_translations(settings.language);

    // 当前调色板名称
    let current_palette_name = settings.selected_palette.as_str();

    // 调色板选择下拉列表
    let palette_options: Vec<&str> = settings.available_palettes.iter().copied().collect();
    let pick_list = pick_list(palette_options, Some(current_palette_name), |name| {
        Message::Settings(Event::PaletteChanged(name.to_string()))
    })
    .width(300.0)
    .style(create_pick_list_style);

    // 获取当前选中调色板的详细信息
    let palette_mgr = &*lumino_core::palette::PALETTE_MANAGER;
    let palette = palette_mgr.get(current_palette_name);

    // 颜色预览
    let preview = if let Some(p) = palette {
        render_palette_swatches(&p.colors)
    } else {
        text(t.palette_no_preview)
            .size(TEXT_SIZE_CONTENT)
            .style(create_placeholder_text_style())
            .into()
    };

    column![
        text(t.palette_title)
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(20),
        // 调色板选择器
        row![
            text(t.palette_select)
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            pick_list,
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        // 说明文字
        text(t.palette_hint)
            .size(12.0)
            .style(create_placeholder_text_style()),
        iced_widget::space().height(20),
        // 颜色预览
        text(t.palette_colors_info)
            .size(TEXT_SIZE_CONTENT)
            .style(create_content_text_style()),
        iced_widget::space().height(10),
        scrollable(
            container(preview)
                .width(Length::Fill)
                .padding(10)
                .style(create_preview_container_style)
        )
        .height(Length::Shrink)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new().width(8).scroller_width(6),
        )),
    ]
    .spacing(SPACING_CONTENT)
    .padding(PADDING_CONTENT)
    .into()
}

/// 渲染调色板的色块网格预览
fn render_palette_swatches<'a>(colors: &[PaletteColor]) -> Element<'a> {
    let mut col = column![].spacing(SWATCH_SPACING);

    for chunk in colors.chunks(SWATCHES_PER_ROW) {
        let mut row = row![].spacing(SWATCH_SPACING);
        for color in chunk {
            let r = color[0] as f32 / 255.0;
            let g = color[1] as f32 / 255.0;
            let b = color[2] as f32 / 255.0;
            let a = color[3] as f32 / 255.0;

            let swatch = container(text("").size(1))
                .width(SWATCH_SIZE)
                .height(SWATCH_SIZE)
                .style(move |_theme: &Theme| container::Style {
                    background: Some(iced_core::Background::Color(iced_core::Color {
                        r,
                        g,
                        b,
                        a,
                    })),
                    border: Border::default()
                        .rounded(3)
                        .width(1)
                        .color(iced_core::Color::from_rgba(0.0, 0.0, 0.0, 0.15)),
                    ..Default::default()
                });

            row = row.push(swatch);
        }
        // 补全剩余空位
        let remain = SWATCHES_PER_ROW - chunk.len();
        if remain > 0 {
            for _ in 0..remain {
                let empty = container(text("").size(1))
                    .width(SWATCH_SIZE)
                    .height(SWATCH_SIZE);
                row = row.push(empty);
            }
        }
        col = col.push(row);
    }

    col.into()
}

/// 预览区域容器样式
fn create_preview_container_style(theme: &Theme) -> container::Style {
    let palette = theme.extended_palette();
    container::Style {
        background: Some(iced_core::Background::Color(palette.background.weak.color)),
        border: Border::default()
            .rounded(BORDER_RADIUS_CONTENT)
            .width(BORDER_WIDTH)
            .color(palette.background.strong.color),
        ..Default::default()
    }
}

/// 调色板下拉列表样式
fn create_pick_list_style(
    theme: &Theme,
    status: iced_widget::pick_list::Status,
) -> iced_widget::pick_list::Style {
    let palette = theme.extended_palette();
    let is_hovered = matches!(
        status,
        iced_widget::pick_list::Status::Hovered | iced_widget::pick_list::Status::Opened { .. }
    );
    iced_widget::pick_list::Style {
        text_color: palette.background.base.text,
        background: palette.background.base.color.into(),
        placeholder_color: palette.background.weak.text,
        handle_color: palette.background.weak.text,
        border: Border::default().rounded(4).width(1).color(if is_hovered {
            palette.primary.strong.color
        } else {
            palette.background.strong.color
        }),
    }
}
