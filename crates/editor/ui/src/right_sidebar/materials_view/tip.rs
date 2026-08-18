//! 素材项悬停提示悬浮面板
//!
//! 文件描述信息（名称/作者/位置/轨道数/来源）在鼠标悬停时于按钮左侧弹出。

use iced_core::widget::text::Wrapping;
use iced_core::{Color, Length};
use iced_widget::{column, container, text};

use lumino_extras::i18n::MainTranslations;

use crate::right_sidebar::MaterialEntry;
use crate::right_sidebar::material::MaterialSource;
use crate::{Element, Theme};

/// 悬停提示悬浮面板宽度（文本行固定宽度，超宽内容自动换行）
const TOOLTIP_WIDTH: f32 = 280.0;

/// 悬停提示悬浮面板内容：文件描述信息，每行均带描述标头
///
/// 显示项：
/// - 名称（metadata.project.name）
/// - 作者（metadata.project.author，素材导出时跟随工程设置面板署名；非空才显示）
/// - 位置（磁盘路径，仅本地素材）
/// - 轨道数（解析到音轨数时显示）
/// - 来源（内置 / 本地）
/// - 无效素材仅显示"素材无效"
pub(super) fn tooltip_content<'a>(
    entry: &'a MaterialEntry,
    t: &'static MainTranslations,
) -> Element<'a> {
    let mut col = column![].spacing(2);

    if !entry.valid {
        col = col.push(text(t.material_invalid).size(10));
        return col.into();
    }

    // 名称
    col = col.push(tooltip_line(format!(
        "{}{}",
        t.material_name_label, entry.name
    )));
    // 作者（跟随工程设置面板的作者栏目在 metadata 中署名）
    if !entry.author.is_empty() {
        col = col.push(tooltip_line(format!(
            "{}{}",
            t.material_author_label, entry.author
        )));
    }
    // 位置（仅本地素材有磁盘路径；长路径自动换行）
    if let Some(path) = &entry.path {
        col = col.push(tooltip_line(format!(
            "{}{}",
            t.material_location_label,
            path.display()
        )));
    }
    // 轨道数
    if entry.track_count > 0 {
        col = col.push(tooltip_line(format!(
            "{}{}",
            t.material_track_label, entry.track_count
        )));
    }
    // 来源
    let source_label = match entry.source {
        MaterialSource::BuiltIn => t.material_section_builtin,
        MaterialSource::User => t.material_section_user,
    };
    col = col.push(tooltip_line(format!(
        "{}{}",
        t.material_source_label, source_label
    )));

    col.into()
}

/// 悬浮窗文本行：固定宽度 + 换行策略
///
/// iced 默认 `Wrapping::Word` 只按单词边界断行，磁盘路径等无空格长文本
/// 视为单个单词永不换行，会撑破悬浮窗——改用 `WordOrGlyph`：
/// 有空格按词断行，超长单词回退到字形级断行。
fn tooltip_line<'a>(content: String) -> Element<'a> {
    text(content)
        .size(10)
        .width(Length::Fixed(TOOLTIP_WIDTH))
        .wrapping(Wrapping::WordOrGlyph)
        .into()
}

/// Tooltip 样式：深色背景 + 浅色文字
pub(super) fn tooltip_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(iced_core::Background::Color(Color::from_rgba(
            0.08, 0.08, 0.10, 0.96,
        ))),
        border: iced_core::Border::default().rounded(4),
        text_color: Some(Color::from_rgba(0.95, 0.95, 0.95, 1.0)),
        ..Default::default()
    }
}
