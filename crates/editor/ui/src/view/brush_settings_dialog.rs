//! 画刷「绘制行为」对话框视图
//!
//! 渲染分层音轨分配列表（粗细度 1..N），每层一行：名称 + 音轨下拉（默认自动/各普通音轨），
//! 行尾圆形 +/- SVG 按钮用于插入/删除层。底部「保存」「取消」。无可用音轨时整列禁用。

use iced_core::Length;
use iced_widget::{button, column, container, pick_list, row, scrollable, space, text};
use lumino_core::BrushConfig;
use lumino_extras::i18n::{Language, main_translations};

use crate::Element;
use crate::message::{BrushSettingsAction, Message};
use crate::resources::icon;

/// 音轨选择项（用于在 pick_list 中区分「默认（自动分配）」与具体音轨）
#[derive(Debug, Clone)]
struct TrackChoice {
    /// 音轨 id；None 表示默认（自动分配）
    id: Option<usize>,
    /// 展示名称
    name: String,
}

impl PartialEq for TrackChoice {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for TrackChoice {}

impl std::fmt::Display for TrackChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

/// 渲染画刷「绘制行为」对话框
pub fn view_brush_settings_dialog<'a>(
    draft: &'a BrushConfig,
    tracks: &'a [(usize, String)],
    theme: &'a iced_core::Theme,
    _language: Language,
) -> Element<'a> {
    let t = main_translations(_language);
    let palette = theme.extended_palette();
    let _ = &t; // 预留后续文案本地化

    let no_tracks = tracks.is_empty();

    // 构建每一层的行
    let mut level_rows: Vec<Element<'a>> = Vec::new();
    for level in 0..draft.tracks.len() {
        let label = format!("粗细度 {}", level + 1);
        let assigned = draft.tracks.get(level).copied().flatten();
        let can_remove = draft.tracks.len() > BrushConfig::MIN_THICKNESS as usize;

        if no_tracks {
            // 无可用音轨：整行灰色禁用提示
            level_rows.push(
                container(row![
                    text(label).size(14),
                    space().width(8),
                    text("（无可用音轨）")
                        .size(12)
                        .style(move |_t: &iced_core::Theme| text::Style {
                            color: Some(palette.background.weak.text),
                        }),
                ])
                .into(),
            );
            continue;
        }

        // 下拉选项：默认（自动分配）+ 每个普通音轨
        let mut options: Vec<TrackChoice> = vec![TrackChoice {
            id: None,
            name: "默认（自动分配）".to_string(),
        }];
        for (id, name) in tracks.iter() {
            options.push(TrackChoice {
                id: Some(*id),
                name: name.clone(),
            });
        }
        let selected = TrackChoice {
            id: assigned,
            name: match assigned {
                Some(id) => tracks
                    .iter()
                    .find(|(i, _)| *i == id)
                    .map(|(_, n)| n.clone())
                    .unwrap_or_else(|| format!("音轨 {}", id)),
                None => "默认（自动分配）".to_string(),
            },
        };

        let level_idx = level;
        level_rows.push(
            container(row![
                text(label).size(14).width(Length::Fixed(64.0)),
                space().width(8),
                pick_list(options, Some(selected), move |c| {
                    Message::BrushSettings(BrushSettingsAction::LevelTrackChanged(level_idx, c.id))
                })
                .width(Length::Fixed(200.0)),
                space().width(8),
                // + 按钮：在当前层下方插入新层
                button(icon::view_with_size_and_theme(
                    icon::PlusCircle,
                    18,
                    18,
                    Some(theme)
                ))
                .on_press(Message::BrushSettings(BrushSettingsAction::AddLevel(
                    level_idx
                )))
                .padding(2),
                space().width(4),
                // - 按钮：删除当前层（仅剩 1 层时禁用）
                button(icon::view_with_size_and_theme(
                    icon::MinusCircle,
                    18,
                    18,
                    Some(theme)
                ))
                .on_press_maybe(if can_remove {
                    Some(Message::BrushSettings(BrushSettingsAction::RemoveLevel(
                        level_idx,
                    )))
                } else {
                    None
                })
                .padding(2),
            ])
            .into(),
        );
    }

    let list: Element<'a> = if no_tracks {
        column![text("无可用音轨，请先在工程中创建普通音轨。").size(14)].into()
    } else {
        column(level_rows).spacing(8).into()
    };

    let content = column![
        text("画刷绘制行为设置").size(18),
        space().height(8),
        scrollable(list).height(Length::Fill).width(Length::Fill),
        space().height(8),
        row![
            button(text("保存").size(14))
                .on_press(Message::BrushSettings(BrushSettingsAction::Save))
                .padding([6, 16]),
            space().width(8),
            button(text("取消").size(14))
                .on_press(Message::BrushSettings(BrushSettingsAction::Cancel))
                .padding([6, 16]),
        ]
        .spacing(8),
    ]
    .spacing(8)
    .padding(16)
    .width(Length::Fill)
    .height(Length::Fill);

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|theme: &iced_core::Theme| container::Style {
            background: Some(iced_core::Background::Color(theme.palette().background)),
            ..Default::default()
        })
        .into()
}
