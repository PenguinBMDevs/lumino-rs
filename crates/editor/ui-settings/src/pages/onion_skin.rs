//! 设置页面 - 洋葱皮设置

use iced_core::Alignment;
use iced_widget::{column, pick_list, row, text};
use lumino_core::storage::config::OnionSkinBackend;
use lumino_core::tr;
use lumino_ui_core::{Element, Message};

use super::super::components::constants::*;
use super::super::components::styles::{create_content_text_style, create_placeholder_text_style};
use crate::SettingsPanel;

/// 可选渲染后端（顺序即下拉顺序：默认 GPU 在首）。
///
/// 用 `static` 而非 `const`：`pick_list` 需要 `&'a [T]`，`static` 的切片借用是
/// `'static`，不会在函数返回时随临时值一起被丢弃。
static BACKEND_OPTIONS: [OnionSkinBackend; 2] = [OnionSkinBackend::Gpu, OnionSkinBackend::Cpu];

/// 「洋葱皮渲染方式」行（GPU / CPU）。
///
/// - 未锁定（空白工程 / 尚未加载 MIDI）：下拉选择，发
///   [`crate::Event::OnionSkinBackendChanged`]；
/// - 已锁定（主窗口已加载真实 MIDI/工程）：只读展示当前后端 + 提示，**不提供可操作控件**。
///
/// 锁定语义的唯一真源是主窗口（`Host::onion_backend_locked`，判的是「是否加载过真实
/// MIDI」而非「有没有文档」——空白工程不是加载 MIDI），面板只镜像。
fn renderer_row<'a>(settings: &SettingsPanel) -> Element<'a> {
    let label = text(tr!("洋葱皮渲染方式"))
        .size(TEXT_SIZE_CONTENT)
        .style(create_content_text_style());

    if settings.onion_skin.backend_locked {
        return row![
            label,
            iced_widget::space().width(SPACING_MAIN),
            text(settings.onion_skin.backend.to_string())
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
            Some(settings.onion_skin.backend),
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
    ]
    .spacing(SPACING_CONTENT)
    .padding(PADDING_CONTENT)
    .into()
}
