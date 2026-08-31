//! 设置搜索 — yinhe `dialogs/settings/search.rs:97` + `constants` 的 iced 迁移桩

use iced_core::Length;
use iced_widget::{button, column, container, row, text};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

use super::constants::{CATEGORY_KEYS, SETTING_ITEMS};

fn norm(s: &str) -> String {
    s.to_lowercase()
}

fn item_matches_query(item: &super::constants::SettingItem, query: &str) -> bool {
    let q = norm(query);
    [item.zh, item.en, item.ja, item.ko]
        .iter()
        .any(|name| norm(name).contains(&q))
}

/// 渲染搜索结果
pub fn view<'a>(window: &'a Window, query: &'a str) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = palette.background.base.color;
    let weak = palette.background.weak.color;

    let q = query.trim();
    let matched: Vec<_> = SETTING_ITEMS
        .iter()
        .filter(|it| item_matches_query(it, q))
        .collect();

    if matched.is_empty() {
        return container(text("no results").size(12).style(move |_t: &Theme| {
            iced_widget::text::Style {
                color: Some(palette.background.weak.text),
            }
        }))
        .width(Length::Fill)
        .padding(12)
        .style(move |_t: &Theme| container::Style {
            background: Some(iced_core::Background::Color(bg)),
            ..Default::default()
        })
        .into();
    }

    let rows: Vec<Element<'a>> = matched
        .into_iter()
        .map(|item| {
            let cat_label = CATEGORY_KEYS[item.cat];
            row![
                container(text(item.zh).size(12))
                    .padding([4, 8])
                    .style(move |_t: &Theme| container::Style {
                        background: Some(iced_core::Background::Color(weak.scale_alpha(0.5))),
                        border: iced_core::Border {
                            radius: 4.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
                text(format!("{} / {} / {}", item.en, item.ja, item.ko))
                    .size(10)
                    .style(move |_t: &Theme| iced_widget::text::Style {
                        color: Some(palette.background.weak.text),
                    }),
                iced_widget::Space::new().width(Length::Fill),
                button(text(cat_label).size(10))
                    .on_press(lumino_ui_core::message::null())
                    .padding([4, 6]),
            ]
            .spacing(8)
            .into()
        })
        .collect();

    container(column(rows).spacing(6).padding(8))
        .width(Length::Fill)
        .style(move |_t: &Theme| container::Style {
            background: Some(iced_core::Background::Color(bg)),
            ..Default::default()
        })
        .into()
}
