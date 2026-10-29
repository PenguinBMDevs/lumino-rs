//! 设置页面 - 洋葱皮设置

use iced_core::Alignment;
use iced_widget::{column, pick_list, row, text, text_input};
use lumino_core::storage::config::OnionSkinBackend;
use lumino_core::tr;
use lumino_ui_core::{Element, Message};

use super::super::components::constants::*;
use super::super::components::styles::{create_content_text_style, create_placeholder_text_style};
use super::super::components::{with_setting_tooltip_inline, with_setting_tooltip_inline_action};
use crate::SettingsPanel;

/// 可选渲染后端（顺序即下拉顺序：默认 GPU 在首）。
///
/// 用 `static` 而非 `const`：`pick_list` 需要 `&'a [T]`，`static` 的切片借用是
/// `'static`，不会在函数返回时随临时值一起被丢弃。
static BACKEND_OPTIONS: [OnionSkinBackend; 2] = [OnionSkinBackend::Gpu, OnionSkinBackend::Cpu];

/// 「洋葱皮渲染方式」行（GPU / CPU）。
///
/// - 未锁定（当前没有 MIDI 工程）：下拉选择，发
///   [`crate::Event::OnionSkinBackendChanged`]；
/// - 已锁定（主窗口已加载 MIDI）：只读展示当前后端 + 提示，**不提供可操作控件**。
///
/// 锁定语义的唯一真源是主窗口（`Host::onion_backend_locked`），面板只镜像。
fn renderer_row<'a>(settings: &SettingsPanel) -> Element<'a> {
    let label = text(tr!("洋葱皮渲染方式"))
        .size(TEXT_SIZE_CONTENT)
        .style(create_content_text_style());

    if settings.hires.backend_locked {
        return row![
            label,
            iced_widget::space().width(SPACING_MAIN),
            text(settings.hires.backend.to_string())
                .size(TEXT_SIZE_CONTENT)
                .style(create_content_text_style()),
            iced_widget::space().width(SPACING_MAIN),
            text(tr!("加载 MIDI 后不可切换，请先关闭 MIDI 工程"))
                .size(TEXT_SIZE_HINT)
                .style(create_placeholder_text_style()),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center)
        .into();
    }

    row![
        label,
        iced_widget::space().width(SPACING_MAIN),
        pick_list(
            BACKEND_OPTIONS.as_slice(),
            Some(settings.hires.backend),
            |backend| Message::Settings(crate::Event::OnionSkinBackendChanged(backend)),
        )
        .width(120.0),
    ]
    .spacing(SPACING_ICON_LABEL)
    .align_y(Alignment::Center)
    .into()
}

/// 渲染洋葱皮设置页面
pub fn view<'a>(settings: &SettingsPanel) -> Element<'a> {
    column![
        text(tr!("洋葱皮设置"))
            .size(TEXT_SIZE_TITLE)
            .style(create_content_text_style()),
        iced_widget::space().height(20),
        // 洋葱皮渲染方式（加载 MIDI 后锁定，见 `Host::onion_backend_locked`）
        renderer_row(settings),
        // CPU 后端的能力边界：入口禁用由 Step 4b 实现，本步先给出提示，
        // 避免用户切到 CPU 后才发现纵向卷帘 / 走带 / 瀑布流 / 视频导出不可用。
        text(tr!("CPU 模式会禁用纵向卷帘、工程走带、瀑布流与视频导出"))
            .size(TEXT_SIZE_HINT)
            .style(create_placeholder_text_style()),
        iced_widget::space().height(24),
        // 启用高精度贴图开关（说明挂在标签文字上，提示不覆盖对钩框；点击文字等价于切换对钩）
        row![
            iced_widget::Checkbox::new(settings.hires.onion_enabled).on_toggle(|enabled| {
                Message::Settings(crate::Event::HiresOnionEnabledChanged(enabled))
            }),
            with_setting_tooltip_inline_action(
                text(tr!("启用高精度洋葱皮贴图"))
                    .size(TEXT_SIZE_CONTENT)
                    .style(create_content_text_style()),
                tr!("启用后将在钢琴卷帘上叠加高精度预渲染贴图，编辑后自动重生成。"),
                Message::Settings(crate::Event::HiresOnionEnabledChanged(
                    !settings.hires.onion_enabled,
                )),
            ),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(24),
        // 组内小节数
        row![
            with_setting_tooltip_inline(
                text(tr!("每组小节数 (1-16)"))
                    .size(TEXT_SIZE_CONTENT)
                    .style(create_content_text_style()),
                tr!("组内小节数越大单张贴图越长，内存占用越高。"),
            ),
            iced_widget::space().width(SPACING_MAIN),
            text_input("4", &settings.hires.measures_per_group.to_string())
                .on_input(|v| Message::Settings(crate::Event::HiresMeasuresPerGroupChanged(v)))
                .width(80.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        // 贴图宽度
        row![
            with_setting_tooltip_inline(
                text(tr!("贴图宽度像素 (480-7680)"))
                    .size(TEXT_SIZE_CONTENT)
                    .style(create_content_text_style()),
                tr!("贴图宽度越大越清晰但内存占用越高，建议不超过 3840。"),
            ),
            iced_widget::space().width(SPACING_MAIN),
            text_input("1920", &settings.hires.tile_width_px.to_string())
                .on_input(|v| Message::Settings(crate::Event::HiresTileWidthChanged(v)))
                .width(80.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        // 冷静期秒数
        row![
            with_setting_tooltip_inline(
                text(tr!("编辑后重生成冷静期秒数 (3-60)"))
                    .size(TEXT_SIZE_CONTENT)
                    .style(create_content_text_style()),
                tr!("停止编辑后等待指定秒数再重生成贴图，避免频繁重绘。"),
            ),
            iced_widget::space().width(SPACING_MAIN),
            text_input("10", &settings.hires.cooldown_secs.to_string())
                .on_input(|v| Message::Settings(crate::Event::HiresCooldownChanged(v)))
                .width(80.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
        iced_widget::space().height(SPACING_CONTENT),
        // GPU 显存上限
        row![
            with_setting_tooltip_inline(
                text(tr!("GPU 显存上限 MB (128-4096)"))
                    .size(TEXT_SIZE_CONTENT)
                    .style(create_content_text_style()),
                tr!("GPU 显存上限越大可同时显示越多贴图。"),
            ),
            iced_widget::space().width(SPACING_MAIN),
            text_input("512", &settings.hires.gpu_mem_limit_mb.to_string())
                .on_input(|v| Message::Settings(crate::Event::HiresGpuMemLimitChanged(v)))
                .width(80.0),
        ]
        .spacing(SPACING_ICON_LABEL)
        .align_y(Alignment::Center),
    ]
    .spacing(SPACING_CONTENT)
    .padding(PADDING_CONTENT)
    .into()
}
