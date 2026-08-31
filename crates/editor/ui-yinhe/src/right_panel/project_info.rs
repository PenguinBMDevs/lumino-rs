//! 工程信息面板 — 对应 yinhe `right_panel/project_info.rs:201`
//!
//! 编辑工程元数据：标题 / 艺术家 / PPQ / 压缩等级 / 描述。
//! yinhe 原用 `TextEdit::singleline / multiline + DragValue + begin_edit/commit_*`
//! 实现历史与 PPQ 重采样确认；iced 桩以 `column + text_input + container` 重构，
//! 保留字段与范围语义，编辑走 `Message` 单向流，PPQ 越界由 Host 弹确认框。

use iced_core::Length;
use iced_widget::{column, container, text, text_input};

use lumino_ui_core::{Element, Theme, window::Window};

/// 工程信息状态（对齐 yinhe `Document.data.model.meta` 精简）
///
/// `ppq` 范围 1..=32767，`compression_level` 0..=22（zstd）。
#[derive(Debug, Clone)]
pub struct ProjectInfoState {
    pub name: String,
    pub artist: String,
    pub ppq: u32,
    pub compression_level: i32,
    pub description: String,
    pub has_notes: bool,
}

impl Default for ProjectInfoState {
    fn default() -> Self {
        Self {
            name: "Untitled".to_string(),
            artist: String::new(),
            ppq: 480,
            compression_level: 3,
            description: String::new(),
            has_notes: false,
        }
    }
}

fn field_label<'a>(window: &'a Window, label: &'a str) -> Element<'a> {
    let palette = window.theme.extended_palette();
    text(label)
        .size(11)
        .style(move |_theme: &Theme| iced_widget::text::Style {
            color: Some(palette.background.weak.text),
        })
        .into()
}

/// 工程信息 `view()` — 五字段纵向表单
///
/// ```text
/// column![
///   label("Project name") + text_input,
///   label("Artist") + text_input,
///   label("PPQ") + number(1..32767),
///   label("Compression") + number(0..22),
///   label("Description") + text_input(multiline 占位),
/// ]
/// ```
pub fn view<'a>(window: &'a Window, state: &'a ProjectInfoState) -> Element<'a> {
    let palette = window.theme.extended_palette();

    let name_field = column![
        field_label(window, "Project name"),
        text_input("name…", &state.name).padding([4, 6]).size(12),
    ]
    .spacing(4);

    let artist_field = column![
        field_label(window, "Artist"),
        text_input("artist…", &state.artist)
            .padding([4, 6])
            .size(12),
    ]
    .spacing(4);

    let ppq_field = column![
        field_label(window, "PPQ"),
        container(
            text(state.ppq.to_string())
                .size(12)
                .style(move |_theme: &Theme| {
                    iced_widget::text::Style {
                        color: Some(palette.background.base.text),
                    }
                })
        )
        .padding([4, 6])
        .width(Length::Fixed(80.0))
        .style(move |_theme: &Theme| container::Style {
            background: Some(iced_core::Background::Color(palette.background.weak.color)),
            border: iced_core::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }),
        text("1..32767 — has_notes → confirm rescale")
            .size(9)
            .style(move |_theme: &Theme| {
                iced_widget::text::Style {
                    color: Some(palette.background.weak.text),
                }
            }),
    ]
    .spacing(4);

    let compression_field = column![
        field_label(window, "Compression (zstd)"),
        container(text(state.compression_level.to_string()).size(12))
            .padding([4, 6])
            .width(Length::Fixed(60.0))
            .style(move |_theme: &Theme| container::Style {
                background: Some(iced_core::Background::Color(palette.background.weak.color)),
                border: iced_core::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
        text("0..22").size(9).style(move |_theme: &Theme| {
            iced_widget::text::Style {
                color: Some(palette.background.weak.text),
            }
        }),
    ]
    .spacing(4);

    let desc_field = column![
        field_label(window, "Description"),
        text_input("description…", &state.description)
            .padding([4, 6])
            .size(12),
    ]
    .spacing(4);

    column![
        name_field,
        artist_field,
        ppq_field,
        compression_field,
        desc_field,
    ]
    .spacing(8)
    .padding([8, 8])
    .into()
}

/// PPQ 重采样待确认状态的 iced 侧占位（对齐 yinhe `PPQ_RESCALE_PENDING_ID`）
///
/// yinhe 原写入 `egui::Id::new(PPQ_RESCALE_PENDING_ID)` 的 `(old, new, id)`；
/// iced 桩以纯数据 `PpqRescalePending` 表达，由 Host 弹确认对话框。
#[derive(Debug, Clone, PartialEq)]
pub struct PpqRescalePending {
    pub old_ppq: u32,
    pub new_ppq: u32,
}

#[must_use]
pub fn should_confirm_ppq_rescale(pending: Option<PpqRescalePending>, has_notes: bool) -> bool {
    pending.is_some() && has_notes
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_ui_core::window::Window;

    #[test]
    fn project_info_view_does_not_panic() {
        let window = Window::new("Tokyo Night Storm");
        let state = ProjectInfoState {
            name: "Demo".to_string(),
            artist: "Penguin".to_string(),
            ppq: 480,
            has_notes: true,
            ..Default::default()
        };
        let _el = view(&window, &state);
    }

    #[test]
    fn ppq_confirm_logic() {
        assert!(should_confirm_ppq_rescale(
            Some(PpqRescalePending {
                old_ppq: 480,
                new_ppq: 960
            }),
            true
        ));
        assert!(!should_confirm_ppq_rescale(None, true));
        assert!(!should_confirm_ppq_rescale(
            Some(PpqRescalePending {
                old_ppq: 480,
                new_ppq: 960
            }),
            false
        ));
    }
}
