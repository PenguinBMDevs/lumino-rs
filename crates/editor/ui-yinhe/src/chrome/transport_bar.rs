//! 传输栏 — yinhe `transport_bar.rs:1912` 的 iced 迁移桩
//!
//! 原 yinhe 含文件/编辑/播放菜单、图钉、FollowMode、工具按钮、方向切换、时间码等；
//!
//! P2 桩保留核心走带语义：播放/停止/录音 + 速度/拍号/量化，
//!
//! 复用 `lumino-ui/src/toolbar/buttons.rs:1..217` 的按钮风格（hover 弱背景、圆角 3、透明常态）
//! 与 `lumino-message` 的 `Message::Toolbar` / `Message::Window` 通道。

use iced_core::{Alignment, Length};
use iced_widget::{button, container, row, space, text};

use lumino_core::{NotePrecision, Tool};
use lumino_ui_core::resources::icon;
use lumino_ui_core::toolbar_event::Event as ToolbarEvent;
use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Message, Theme};

/// 传输栏状态（聚合入参，避免 `TransportContext` 长参数列表）
///
/// 对应 yinhe `TransportContext` + `TransportResponse` 的 iced 精简版；
///
/// P2 仅覆盖任务要求的 6 要素：播放/停止/录音/速度/拍号/量化。
#[derive(Debug, Clone)]
pub struct TransportState {
    /// 是否正在播放（`Document.edit.playback.is_playing()`）
    pub is_playing: bool,
    /// 是否正在录制（REC 高亮）
    pub is_recording: bool,
    /// 速度 BPM（显示用；点击可触发展开速度面板，P3 完善）
    pub bpm: f32,
    /// 拍号分子（如 4）
    pub time_sig_numer: u8,
    /// 拍号分母（如 4）
    pub time_sig_denom: u8,
    /// 当前量化精度
    pub quantize: NotePrecision,
    /// 当前工具（高亮用，仅占位，P3 接入 editor_state）
    pub active_tool: Tool,
    /// 是否有活动文档（无文档时部分按钮禁用）
    pub has_active_document: bool,
}

impl Default for TransportState {
    fn default() -> Self {
        Self {
            is_playing: false,
            is_recording: false,
            bpm: 120.0,
            time_sig_numer: 4,
            time_sig_denom: 4,
            quantize: NotePrecision::Quarter,
            active_tool: Tool::Pointer,
            has_active_document: false,
        }
    }
}

// ── 按钮风格：复用 toolbar/buttons.rs 语义 ──

fn transport_button<'a>(
    icon_enum: icon::Icon,
    tooltip: &'static str,
    on_press: Option<Message>,
    is_active: bool,
    window: &'a Window,
) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg_strong = palette.background.strong.color;
    let bg_weak = palette.background.weak.color;

    let icon_el = icon::view_with_size_and_theme(icon_enum, 16, 16, Some(&window.theme));

    let mut btn = button(icon_el)
        .padding(6)
        .style(move |_theme: &Theme, status| {
            let bg = if is_active {
                bg_strong
            } else if status == button::Status::Hovered {
                bg_weak
            } else {
                iced_core::Color::TRANSPARENT
            };
            button::Style {
                background: Some(iced_core::Background::Color(bg)),
                border: iced_core::Border {
                    radius: 3.0.into(),
                    width: 0.0,
                    color: iced_core::Color::TRANSPARENT,
                },
                ..Default::default()
            }
        });

    if let Some(msg) = on_press {
        btn = btn.on_press(msg);
    }

    // tooltip 占位：P2 桩暂以按钮本身保证编译，外层可按需包裹 `iced_widget::tooltip`
    let _ = tooltip;
    btn.into()
}

fn text_button<'a>(
    label: String,
    tooltip: &'static str,
    on_press: Option<Message>,
    is_active: bool,
    window: &'a Window,
) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg_strong = palette.background.strong.color;
    let bg_weak = palette.background.weak.color;

    let txt = text(label).size(12).style(move |theme: &Theme| {
        let p = theme.extended_palette();
        iced_widget::text::Style {
            color: Some(if is_active {
                p.primary.strong.color
            } else {
                p.background.base.text
            }),
        }
    });

    let mut btn = button(txt)
        .padding([4, 8])
        .style(move |_theme: &Theme, status| {
            let bg = if is_active {
                bg_strong
            } else if status == button::Status::Hovered {
                bg_weak
            } else {
                iced_core::Color::TRANSPARENT
            };
            button::Style {
                background: Some(iced_core::Background::Color(bg)),
                border: iced_core::Border {
                    radius: 3.0.into(),
                    width: 0.0,
                    color: iced_core::Color::TRANSPARENT,
                },
                ..Default::default()
            }
        });

    if let Some(msg) = on_press {
        btn = btn.on_press(msg);
    }
    let _ = tooltip;
    btn.into()
}

/// 渲染传输栏
///
/// ```text
/// [▶/⏸] [■] [●REC] | 120.0 BPM | 4/4 | 1/8 | [工具...]
/// ```
/// - 播放/暂停：`Toolbar::Play` / `Toolbar::Pause`（`Message::Toolbar`）
/// - 停止：`Toolbar::Stop`
/// - 录音：`Toolbar::Record` / `RecordStop`（录音中红色高亮，复用 `button` accent 思路）
/// - 速度：`Toolbar::SpeedChange`（占位，点击触发变速面板，P3 完善）
/// - 拍号：占位文本按钮（P3 接入拍号编辑对话框）
/// - 量化：`Toolbar::Quantize` / `PrecisionChanged`（P2 桩以 Quantize 占位）
pub fn view<'a>(window: &'a Window, state: TransportState) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let has_doc = state.has_active_document;

    // 播放/暂停：根据 is_playing 切换图标与消息
    let (play_icon, play_msg, play_tip): (icon::Icon, Message, &'static str) = if state.is_playing {
        (icon::Icon::Pause, ToolbarEvent::pause(), "暂停")
    } else {
        (icon::Icon::Play, ToolbarEvent::play(), "播放")
    };
    let play_btn = transport_button(
        play_icon,
        play_tip,
        has_doc.then_some(play_msg),
        state.is_playing,
        window,
    );

    let stop_btn = transport_button(
        icon::Icon::Ban,
        "停止",
        has_doc.then_some(ToolbarEvent::stop()),
        false,
        window,
    );

    // 录音：高亮 + 红色语义（复用 traffic 关闭按钮的红）
    let record_btn: Element<'a> = {
        let is_rec = state.is_recording;
        let rec_icon = icon::Icon::PlayCircle;
        let palette_weak = palette.background.weak.color;
        let rec_red = iced_core::Color::from_rgb8(220, 38, 38);
        let icon_el = icon::view_with_size_and_theme(rec_icon, 16, 16, Some(&window.theme));
        let msg = if is_rec {
            ToolbarEvent::record_stop()
        } else {
            ToolbarEvent::record()
        };
        let mut btn = button(icon_el)
            .padding(6)
            .style(move |_theme: &Theme, status| {
                let bg = if is_rec {
                    rec_red
                } else if status == button::Status::Hovered {
                    palette_weak
                } else {
                    iced_core::Color::TRANSPARENT
                };
                button::Style {
                    background: Some(iced_core::Background::Color(bg)),
                    border: iced_core::Border {
                        radius: 3.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            });
        if has_doc {
            btn = btn.on_press(msg);
        }
        let _ = "录制";
        btn.into()
    };

    // 速度、拍号、量化：文本按钮（复用 toolbar/buttons 风格）
    let bpm_label = format!("{:.1} BPM", state.bpm);
    let bpm_btn = text_button(
        bpm_label,
        "速度",
        has_doc.then_some(ToolbarEvent::speed_change()),
        false,
        window,
    );

    let sig_label = format!("{}/{}", state.time_sig_numer, state.time_sig_denom);
    let sig_btn = text_button(
        sig_label, "拍号",
        // P3 接入拍号编辑；P2 桩以 Null 占位或复用 PpqEdit 语义
        None, false, window,
    );

    let quant_label = state.quantize.display_name().to_string();
    let quant_btn = text_button(
        quant_label,
        "量化",
        has_doc.then_some(ToolbarEvent::quantize()),
        false,
        window,
    );

    // 工具按钮占位（复用 toolbar 工具选择语义，P3 接入真实 Tool 状态）
    let tool_btn = transport_button(
        match state.active_tool {
            Tool::Pointer => icon::Icon::MousePointer,
            Tool::PointerYSelect => icon::Icon::MousePointerYSelect,
            Tool::Pencil => icon::Icon::Pencil,
            Tool::Eraser | Tool::DrawEraser => icon::Icon::Eraser,
            Tool::Brush => icon::Icon::BrushTool,
            Tool::Curve => icon::Icon::Curve,
            Tool::Shape => icon::Icon::ShapeTool,
            Tool::Text => icon::Icon::TextInput,
            Tool::Pen => icon::Icon::Pencil,
            Tool::Razor => icon::Icon::Eraser,
        },
        "工具",
        has_doc.then_some(ToolbarEvent::tool_selected(state.active_tool)),
        true,
        window,
    );

    let content = row![
        play_btn,
        stop_btn,
        record_btn,
        space().width(12),
        container(row![].spacing(0))
            .width(1)
            .height(24)
            .style(|theme: &Theme| {
                let p = theme.extended_palette();
                container::Style {
                    background: Some(iced_core::Background::Color(p.background.weak.color)),
                    ..Default::default()
                }
            }),
        space().width(12),
        bpm_btn,
        space().width(8),
        sig_btn,
        space().width(8),
        quant_btn,
        space().width(12),
        tool_btn,
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .padding([4, 8]);

    container(content)
        .width(Length::Fill)
        .height(Length::Fixed(40.0))
        .style(|theme: &Theme| {
            let p = theme.extended_palette();
            container::Style {
                background: Some(iced_core::Background::Color(p.background.base.color)),
                ..Default::default()
            }
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_state_default() {
        let s = TransportState::default();
        assert!(!s.is_playing);
        assert!(!s.is_recording);
        assert_eq!(s.bpm, 120.0);
        assert_eq!(s.time_sig_numer, 4);
        assert_eq!(s.time_sig_denom, 4);
    }

    #[test]
    fn transport_state_quantize_display() {
        let s = TransportState {
            quantize: NotePrecision::Eighth,
            ..Default::default()
        };
        assert_eq!(s.quantize.display_name(), "八分音符");
    }
}
