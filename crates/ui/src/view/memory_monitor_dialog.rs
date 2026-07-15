//! 内存监控对话框视图
//!
//! 移植自 yinhe-egui 的 memory_breakdown.rs，使用 iced 渲染。
//! 每次 view 被调用时重新 capture Snapshot，利用 dialog 窗口的 ~16ms redraw 频率实现实时刷新。

use iced_core::Length;
use iced_widget::{button, column, container, row, space, text};
use lumino_memtrace::{AllocTag, Snapshot};

use crate::message::Message;
use crate::state::root_state::MemoryMonitorDialogState;

/// 渲染内存监控对话框
pub fn view_memory_monitor_dialog<'a>(
    _state: &'a MemoryMonitorDialogState,
    theme: &'a iced_core::Theme,
) -> crate::Element<'a> {
    let palette = theme.extended_palette();
    let snapshot = Snapshot::capture();
    let rss_mb = lumino_memory_monitor::platform::get_current_rss() as f64 / 1_048_576.0;

    let label_style = move |_theme: &iced_core::Theme| text::Style {
        color: Some(palette.background.neutral.text),
    };
    let value_style = move |_theme: &iced_core::Theme| text::Style {
        color: Some(palette.background.strong.text),
    };
    let accent_style = move |_theme: &iced_core::Theme| text::Style {
        color: Some(palette.primary.strong.color),
    };
    let note_style = move |_theme: &iced_core::Theme| text::Style {
        color: Some(palette.background.weak.text),
    };

    // 标题
    let title = text("内存占用详情")
        .size(18)
        .style(move |_theme: &iced_core::Theme| text::Style {
            color: Some(palette.background.neutral.text),
        });

    // 总体指标
    let total_row = metric_row(
        "分配器追踪总内存",
        format!("{:.1} MB", snapshot.total_mb()),
        label_style,
        accent_style,
    );
    let rss_row = metric_row(
        "系统 RSS",
        format!("{:.1} MB", rss_mb),
        label_style,
        accent_style,
    );
    let gpu_row = metric_row(
        "GPU 资源",
        format!("{:.1} MB", snapshot.gpu_mb()),
        label_style,
        accent_style,
    );

    // 按子系统分类网格
    let grid_title = text("按子系统分类").size(14).style(label_style);

    let mut grid_rows = column![].spacing(4);
    for tag in AllocTag::ALL {
        // 跳过占用为 0 的子系统，避免显示无意义行（Other 兜底桶通常非空）
        if snapshot.get(tag) <= 0 {
            continue;
        }
        let name = text(tag.name()).size(13).style(label_style);
        let value = text(format!("{:.1} MB", snapshot.mb(tag)))
            .size(13)
            .style(value_style);
        grid_rows = grid_rows.push(
            row![name.width(Length::Fill), value.width(Length::Shrink),]
                .align_y(iced_core::Alignment::Center),
        );
    }

    // 底部说明
    let note = text(
        "注：GPU 资源计数反映应用显式创建的 wgpu Texture/Buffer 大小；\
         驱动层额外开销（swapchain、depth、pipeline cache 等）不纳入此项统计。",
    )
    .size(11)
    .style(note_style);

    // 关闭按钮：通过 Core 事件通道将关闭请求发往 Runner
    let close_button = button(text("关闭").size(14))
        .on_press(Message::Core(crate::event::Event::Window(
            crate::event::window::Event::close_memory_monitor_dialog(),
        )))
        .padding([8, 32])
        .width(Length::Fixed(100.0))
        .style(move |_theme: &iced_core::Theme, status| {
            let bg = match status {
                button::Status::Hovered => palette.primary.strong.color,
                _ => palette.primary.base.color,
            };
            button::Style {
                background: Some(bg.into()),
                text_color: iced_core::Color::WHITE,
                border: iced_core::Border {
                    radius: 4.0.into(),
                    width: 0.0,
                    color: iced_core::Color::TRANSPARENT,
                },
                snap: false,
                shadow: Default::default(),
            }
        });

    let content = column![
        title,
        space().height(16),
        total_row,
        space().height(4),
        rss_row,
        space().height(4),
        gpu_row,
        space().height(16),
        grid_title,
        space().height(8),
        grid_rows,
        space().height(16),
        note,
        space().height(16),
        close_button,
    ]
    .align_x(iced_core::Alignment::Start)
    .spacing(4);

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(24)
        .style(move |_theme: &iced_core::Theme| {
            container::Style::default().background(palette.background.base.color)
        })
        .into()
}

/// 单行指标（标签 + 数值）
fn metric_row<'a>(
    label: &'static str,
    value: String,
    label_style: impl Fn(&iced_core::Theme) -> text::Style + 'a,
    value_style: impl Fn(&iced_core::Theme) -> text::Style + 'a,
) -> crate::Element<'a> {
    row![
        text(label).size(14).style(label_style),
        space().width(Length::Fill),
        text(value).size(14).style(value_style),
    ]
    .align_y(iced_core::Alignment::Center)
    .into()
}
