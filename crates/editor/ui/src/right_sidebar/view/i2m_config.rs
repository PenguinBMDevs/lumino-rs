use super::*;

use lumino_core::tr;

use crate::right_sidebar::i2m_tables::{
    blend_modes, color_id_methods, key_modes, note_length_modes, resize_algorithms,
};

/// 转换参数配置区：key 范围、尺寸、颜色数、颜色后处理与各算法选择
pub(super) fn build_config_section<'a>(right_sidebar: &'a RightSidebar) -> Element<'a> {
    let cfg = &right_sidebar.config;

    Column::new()
        .spacing(4)
        .push(section_label(tr!("转换参数")))
        .push(config_row(
            tr!("Key 范围"),
            Row::new()
                .push(config_input(&cfg.start_key_text, I2mConfigField::StartKey))
                .push(iced_widget::text("~").size(12).style(|theme: &Theme| {
                    iced_widget::text::Style {
                        color: Some(theme.extended_palette().background.strong.text),
                    }
                }))
                .push(config_input(&cfg.end_key_text, I2mConfigField::EndKey))
                .spacing(4)
                .align_y(Alignment::Center)
                .into(),
        ))
        .push(config_row(
            tr!("目标高度"),
            config_input(&cfg.target_height_text, I2mConfigField::TargetHeight),
        ))
        .push(config_row(
            tr!("每像素 tick"),
            config_input(&cfg.ticks_per_pixel_text, I2mConfigField::TicksPerPixel),
        ))
        .push(config_row(
            tr!("颜色数"),
            config_input(&cfg.color_count_text, I2mConfigField::ColorCount),
        ))
        .push(config_row(
            tr!("最长音符"),
            config_input(&cfg.max_note_length_text, I2mConfigField::MaxNoteLength),
        ))
        .push(section_label(tr!("观感调整")))
        .push(config_row(
            tr!("亮度"),
            config_input(&cfg.brightness_text, I2mConfigField::Brightness),
        ))
        .push(config_row(
            tr!("对比度"),
            config_input(&cfg.contrast_text, I2mConfigField::Contrast),
        ))
        .push(config_row(
            tr!("饱和度"),
            config_input(&cfg.saturation_text, I2mConfigField::Saturation),
        ))
        .push(config_row(
            tr!("锐化"),
            config_input(&cfg.sharpen_text, I2mConfigField::Sharpen),
        ))
        .push(section_label(tr!("算法")))
        .push(config_select(
            tr!("调色板"),
            palette_algorithms()
                .into_iter()
                .enumerate()
                .map(|(index, (_, name, _))| (name, index))
                .collect(),
            cfg.palette_index,
            I2mSelectField::Palette,
        ))
        .push(config_select(
            tr!("混合模式"),
            blend_modes()
                .into_iter()
                .enumerate()
                .map(|(index, (_, name, _))| (name, index))
                .collect(),
            cfg.blend_index,
            I2mSelectField::Blend,
        ))
        .push(config_select(
            tr!("key 映射"),
            key_modes()
                .into_iter()
                .enumerate()
                .map(|(index, (_, name, _))| (name, index))
                .collect(),
            cfg.key_mode_index,
            I2mSelectField::KeyMode,
        ))
        .push(config_select(
            tr!("音符长度"),
            note_length_modes()
                .into_iter()
                .enumerate()
                .map(|(index, (_, name, _))| (name, index))
                .collect(),
            cfg.note_length_mode_index,
            I2mSelectField::NoteLengthMode,
        ))
        .push(config_select(
            tr!("重采样"),
            resize_algorithms()
                .into_iter()
                .enumerate()
                .map(|(index, (_, name, _))| (name, index))
                .collect(),
            cfg.resize_algorithm_index,
            I2mSelectField::ResizeAlgorithm,
        ))
        .push(config_select(
            tr!("颜色匹配"),
            color_id_methods()
                .into_iter()
                .enumerate()
                .map(|(index, (_, name, _))| (name, index))
                .collect(),
            cfg.color_id_method_index,
            I2mSelectField::ColorIdMethod,
        ))
        .into()
}

/// 小节标题（跟随主题：暗色白、亮色黑，与项目内面板标题一致）
fn section_label<'a>(title: &'a str) -> Element<'a> {
    iced_widget::text(title)
        .size(12)
        .style(|theme: &Theme| iced_widget::text::Style {
            color: Some(theme.extended_palette().background.neutral.text),
        })
        .into()
}

/// 单行配置：标签居左（文字色跟随主题），控件居右
fn config_row<'a>(label: &'a str, control: Element<'a>) -> Element<'a> {
    Row::new()
        .push(
            iced_widget::text(label)
                .size(12)
                .style(|theme: &Theme| iced_widget::text::Style {
                    color: Some(theme.extended_palette().background.strong.text),
                }),
        )
        .push(Space::new().width(Length::Fill))
        .push(control)
        .spacing(4)
        .align_y(Alignment::Center)
        .into()
}

/// 单行下拉：`items` 为 `(显示名, 索引)`，选中项按索引反查
fn config_select<'a>(
    label: &'a str,
    items: Vec<(&'static str, usize)>,
    current_index: usize,
    field: I2mSelectField,
) -> Element<'a> {
    let names: Vec<&'static str> = items.iter().map(|(name, _)| *name).collect();
    let selected = items
        .iter()
        .find(|(_, index)| *index == current_index)
        .map(|(name, _)| *name)
        .or_else(|| names.first().copied());
    let lookup = items;
    let control = iced_widget::pick_list(names, selected, move |name: &'static str| {
        let index = lookup
            .iter()
            .find(|(candidate, _)| *candidate == name)
            .map(|(_, index)| *index)
            .unwrap_or(0);
        Message::RightSidebar(RightSidebarAction::I2mSelectionChanged { field, index })
    })
    .text_size(12)
    .padding([3, 6])
    .width(Length::Fixed(118.0));
    config_row(label, control.into())
}

/// 小型数字输入框（带边框，仅接受数字/负号）
fn config_input<'a>(value: &'a str, field: I2mConfigField) -> Element<'a> {
    container(
        iced_widget::text_input("", value)
            .on_input(move |text| {
                Message::RightSidebar(RightSidebarAction::I2mConfigTextChanged { field, text })
            })
            .padding([3, 6])
            .size(iced_core::Pixels(12.0))
            .width(Length::Fixed(46.0)),
    )
    .style(|theme: &Theme| {
        let palette = theme.extended_palette();
        container::Style {
            background: Some(palette.background.weak.color.into()),
            border: iced_core::Border {
                radius: 4.0.into(),
                width: 1.0,
                color: palette.background.strong.color,
            },
            ..Default::default()
        }
    })
    .into()
}
