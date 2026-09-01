//! Mode 栏 — yinhe `ViewMode` 的 iced 迁移桩
//!
//! 原 `yinhe-egui/src/chrome/mode_bar.rs:1..324`（egui + hover_button）
//! 在 P2 迁移为 iced_widget::row + button，复用 lumino 主题/字体/图标。
//!
//! - yinhe `ViewMode::Arrange / Mix / Edit` 在 lumino 侧改名 `Piano`（Edit=钢琴卷帘）
//!   以避免与 lumino `AppMode::Editor` 混淆，保留 3 档语义：
//!   `Arrange`（走带）/ `Piano`（钢琴卷帘）/ `Mix`（混音台）
//! - `lumino AppMode::Yinhe` 为进入 yinhe 副模式的顶层开关；
//!   本栏仅在 `AppMode::Yinhe` 下高亮，其余模式仅作展示（避免跨 P 状态耦合）
//! - 右侧预留 CPU/MEM/FPS 指标（复用 lumino `statusbar::performance::PerfData` 样式）
//!   当前以 stub 文本占位，P3 接入真实 perf 信号

use iced_core::{Alignment, Length};
use iced_widget::{button, container, row, space, text};
use serde::{Deserialize, Serialize};

use lumino_message::{YinheAction, YinheViewMode};
use lumino_ui_core::app_mode::AppMode;
use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Message, Theme};

/// Yinhe 视图模式 — 对应 yinhe `ViewMode`（`mode_bar.rs:8..12`）
///
/// yinhe 原 3 档：`Arrange/Mix/Edit`；lumino 侧 `Edit` 重命名为 `Piano`
/// 以避免与 `AppMode::Editor` 歧义，任务描述写作 `Arrange/Piano/Mix`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ViewMode {
    /// 走带/编排视图（显示 transport + 可选叠加钢琴卷帘）
    #[default]
    Arrange,
    /// 钢琴卷帘（等价 yinhe `Edit`）
    Piano,
    /// 混音台
    Mix,
}

impl ViewMode {
    /// 所有变体（供 pick_list / 轮转测试）
    pub const ALL: [ViewMode; 3] = [ViewMode::Arrange, ViewMode::Piano, ViewMode::Mix];

    /// 展示名（按钮文本）
    pub fn as_str(self) -> &'static str {
        match self {
            ViewMode::Arrange => "ARRANGE",
            ViewMode::Piano => "PIANO",
            ViewMode::Mix => "MIX",
        }
    }

    /// 是否显示走带（transport）区域 — 对应 yinhe `ViewMode::show_transport`
    #[inline]
    pub fn show_transport(self) -> bool {
        matches!(self, ViewMode::Arrange)
    }

    /// 是否显示钢琴卷帘区域
    ///
    /// `show_pianoroll_in_arrange` 为用户偏好：Arrange 模式下是否同时显示钢琴卷帘
    #[inline]
    pub fn show_pianoroll(self, show_pianoroll_in_arrange: bool) -> bool {
        match self {
            ViewMode::Arrange => show_pianoroll_in_arrange,
            ViewMode::Mix => false,
            ViewMode::Piano => true,
        }
    }

    /// 兼容 yinhe 原 `ViewMode::Edit` 命名（将 Edit 视为 Piano 别名）
    #[inline]
    pub fn is_edit_alias(self) -> bool {
        self == ViewMode::Piano
    }
}

/// 模式栏右侧性能指标（stub，占位用，P3 接入真实 PerfData）
#[derive(Debug, Clone, Copy, Default)]
pub struct ModeBarMetrics {
    /// CPU 占用百分比
    pub cpu_usage: f32,
    /// 内存 MB
    pub mem_mb: f64,
    /// FPS
    pub fps: f32,
}

/// 将 `ViewMode` 映射为 `Message::Yinhe`
fn view_mode_message(mode: ViewMode) -> Message {
    let vm = match mode {
        ViewMode::Arrange => YinheViewMode::Arrange,
        ViewMode::Piano => YinheViewMode::Piano,
        ViewMode::Mix => YinheViewMode::Mix,
    };
    Message::Yinhe(YinheAction::ViewModeChanged(vm))
}

/// 单个模式按钮（复用 `lumino-ui/src/toolbar/buttons.rs:tool_selector_custom` 风格）
///
/// 选中态：背景 `palette.background.strong`；常态透明，hover 时 `palette.background.weak`
fn mode_button<'a>(
    label: &'static str,
    is_selected: bool,
    window: &'a Window,
    on_press: Message,
) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg_strong = palette.background.strong.color;
    let bg_weak = palette.background.weak.color;

    let label_el = text(label).size(12).style(move |theme: &Theme| {
        let p = theme.extended_palette();
        iced_widget::text::Style {
            color: Some(if is_selected {
                p.background.strong.text
            } else {
                p.background.base.text
            }),
        }
    });

    button(label_el)
        .on_press(on_press)
        .padding([4, 10])
        .style(move |_theme: &Theme, status| {
            let bg = if is_selected {
                bg_strong
            } else if status == button::Status::Hovered {
                bg_weak
            } else {
                iced_core::Color::TRANSPARENT
            };
            button::Style {
                background: Some(iced_core::Background::Color(bg)),
                text_color: iced_core::Color::WHITE,
                border: iced_core::Border {
                    radius: 3.0.into(),
                    width: 0.0,
                    color: iced_core::Color::TRANSPARENT,
                },
                ..Default::default()
            }
        })
        .into()
}

/// 渲染 mode 栏
///
/// - 左侧：`ARRANGE / PIANO / MIX` 三档（`iced_widget::row + button`）
/// - 中间：可选的钢琴卷帘叠加开关（仅 Arrange 下显示，复用 `icon::Pencil` 占位）
/// - 右侧：性能指标 / 右侧面板切换（Info/SoundFont/EventBrowser）占位，P3 完善
pub fn view<'a>(
    window: &'a Window,
    app_mode: AppMode,
    view_mode: ViewMode,
    show_pianoroll_in_arrange: bool,
    metrics: Option<ModeBarMetrics>,
) -> Element<'a> {
    let palette = window.theme.extended_palette();

    // 顶层 AppMode::Yinhe368 分支提示：非 Yinhe 模式时整栏弱化（但仍渲染，便于 P2 预览）
    let is_yinhe = app_mode == AppMode::Yinhe;

    let arrange_btn = mode_button(
        ViewMode::Arrange.as_str(),
        view_mode == ViewMode::Arrange,
        window,
        view_mode_message(ViewMode::Arrange),
    );
    let piano_btn = mode_button(
        ViewMode::Piano.as_str(),
        view_mode == ViewMode::Piano,
        window,
        view_mode_message(ViewMode::Piano),
    );
    let mix_btn = mode_button(
        ViewMode::Mix.as_str(),
        view_mode == ViewMode::Mix,
        window,
        view_mode_message(ViewMode::Mix),
    );

    let mut left_row = row![arrange_btn, piano_btn, mix_btn]
        .spacing(4)
        .align_y(Alignment::Center);

    // Arrange 下的钢琴卷帘叠加开关（yinhe 原 mode_bar.rs:183..202，ICON_PIANO e521）
    if view_mode == ViewMode::Arrange {
        let pianoroll_btn: Element<'a> = {
            let is_active = show_pianoroll_in_arrange;
            let bg_strong = palette.background.strong.color;
            let bg_weak = palette.background.weak.color;
            let icon_color = if is_active {
                palette.background.strong.text
            } else {
                palette.background.base.text
            };
            let btn = button(crate::material_icons::icon(
                crate::material_icons::codepoints::PIANO,
                14.0,
                icon_color,
            ))
            .on_press(Message::Yinhe(YinheAction::TogglePianorollInArrange))
            .padding(4)
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
                        ..Default::default()
                    },
                    ..Default::default()
                }
                .with_background(bg)
            });
            btn.into()
        };
        left_row = left_row.push(space().width(8)).push(pianoroll_btn);
    }

    // 右侧指标区（复用 statusbar metric 风格：label 弱色、value 强调色）
    let right_section: Element<'a> = if let Some(m) = metrics {
        let label_style = |theme: &Theme| iced_widget::text::Style {
            color: Some(theme.extended_palette().background.weak.text),
        };
        let value_style = |theme: &Theme| iced_widget::text::Style {
            color: Some(theme.extended_palette().primary.strong.color),
        };
        row![
            text("CPU").size(11).style(label_style),
            text(format!("{:.1}%", m.cpu_usage))
                .size(11)
                .style(value_style),
            space().width(8),
            text("MEM").size(11).style(label_style),
            text(format!("{:.1} MB", m.mem_mb))
                .size(11)
                .style(value_style),
            space().width(8),
            text("FPS").size(11).style(label_style),
            text(format!("{:.1}", m.fps)).size(11).style(value_style),
        ]
        .align_y(Alignment::Center)
        .into()
    } else {
        space().width(Length::Shrink).into()
    };

    let bar_content = row![left_row, space().width(Length::Fill), right_section,]
        .align_y(Alignment::Center)
        .padding([0, 8]);

    let bg = if is_yinhe {
        palette.background.weakest.color
    } else {
        palette.background.base.color
    };

    container(bar_content)
        .width(Length::Fill)
        .height(Length::Fixed(28.0))
        .style(move |_theme: &Theme| container::Style {
            background: Some(iced_core::Background::Color(bg)),
            ..Default::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_mode_show_transport() {
        assert!(ViewMode::Arrange.show_transport());
        assert!(!ViewMode::Piano.show_transport());
        assert!(!ViewMode::Mix.show_transport());
    }

    #[test]
    fn view_mode_show_pianoroll() {
        assert!(ViewMode::Piano.show_pianoroll(false));
        assert!(!ViewMode::Mix.show_pianoroll(true));
        assert!(ViewMode::Arrange.show_pianoroll(true));
        assert!(!ViewMode::Arrange.show_pianoroll(false));
    }

    #[test]
    fn view_mode_as_str() {
        assert_eq!(ViewMode::Arrange.as_str(), "ARRANGE");
        assert_eq!(ViewMode::Piano.as_str(), "PIANO");
        assert_eq!(ViewMode::Mix.as_str(), "MIX");
    }
}
