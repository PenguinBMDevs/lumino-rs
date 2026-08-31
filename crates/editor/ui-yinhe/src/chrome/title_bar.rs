//! 标题栏 — yinhe `title_bar.rs:520` 的 iced 迁移桩
//!
//! - yinhe 原为多标签（`Document` 切页 + 拖动排序 + 拖出新建窗口 + paint/paint_tabs）
//! - P2 改为单文档显示 `文件名 + 脏点`（`●`），与 lumino 单工程模型对齐
//! - 复用 `lumino-ui/src/titlebar/traffic.rs:1..119` 的窗口控制按钮样式与 `Window` 状态
//! - macOS 保留 `with_titlebar_transparent + with_fullsize_content_view`（见
//!   `src/runner/window_manager.rs:337..339`），本视图在 macOS 下隐藏自绘红绿灯

use iced_core::{Alignment, Length};
use iced_widget::{button, container, mouse_area, row, space, text};

use lumino_ui_core::window::{TrafficAction, Window};
use lumino_ui_core::{Element, Theme};

/// 标题栏单文档状态
///
/// 对应 yinhe `title_bar.rs:Show` 的 `documents: &[Document]` 多标签；
///
/// P2 单文档：仅文件名 + 脏标记。
#[derive(Debug, Clone, Default)]
pub struct TitleBarState {
    /// 当前文件名（`None` 为未命名/新建工程）
    pub file_name: Option<String>,
    /// 是否有未保存改动（脏点 `●`）
    pub is_dirty: bool,
}

impl TitleBarState {
    /// 构造未命名状态
    pub fn untitled() -> Self {
        Self {
            file_name: None,
            is_dirty: false,
        }
    }

    /// 构造已命名状态
    pub fn named(name: impl Into<String>, is_dirty: bool) -> Self {
        Self {
            file_name: Some(name.into()),
            is_dirty,
        }
    }

    /// 显示文本：文件名 + 脏点（`●`），`None` 时显示 `Untitled`
    pub fn display_name(&self) -> String {
        let base = self.file_name.as_deref().unwrap_or("Untitled");
        if self.is_dirty {
            format!("{} ●", base)
        } else {
            base.to_string()
        }
    }
}

// ── traffic（窗口控制）本地复刻：复用 lumino-ui/src/titlebar/traffic.rs 样式 ──

#[derive(Debug, Clone)]
struct TrafficConfig {
    icon: TrafficIcon,
    color: Option<iced_core::Color>,
    action: TrafficAction,
    tooltip: &'static str,
}

#[derive(Debug, Clone, Copy)]
enum TrafficIcon {
    Static(lumino_ui_core::resources::icon::Icon),
    Toggle {
        normal: lumino_ui_core::resources::icon::Icon,
        active: lumino_ui_core::resources::icon::Icon,
    },
}

const TRAFFICS: &[TrafficConfig] = &[
    TrafficConfig {
        icon: TrafficIcon::Static(lumino_ui_core::resources::icon::Icon::WindowMin),
        color: None,
        action: TrafficAction::Minimize,
        tooltip: "最小化",
    },
    TrafficConfig {
        icon: TrafficIcon::Toggle {
            normal: lumino_ui_core::resources::icon::Icon::WindowMax,
            active: lumino_ui_core::resources::icon::Icon::WindowUnMax,
        },
        color: None,
        action: TrafficAction::ToggleMaximize,
        tooltip: "最大化",
    },
    TrafficConfig {
        icon: TrafficIcon::Static(lumino_ui_core::resources::icon::Icon::WindowClose),
        color: Some(iced_core::Color::from_rgb8(196, 43, 28)),
        action: TrafficAction::Close,
        tooltip: "关闭",
    },
];

fn traffic_item<'a>(cfg: &'a TrafficConfig, window: &'a Window) -> Element<'a> {
    use lumino_ui_core::resources::icon;
    let icon_enum = match cfg.icon {
        TrafficIcon::Static(r) => r,
        TrafficIcon::Toggle { normal, active } => {
            if window.is_maximized {
                active
            } else {
                normal
            }
        }
    };
    let icon_img: Element<'a> =
        icon::view_with_size_and_theme(icon_enum, 10, 10, Some(&window.theme));

    let inner = container(icon_img)
        .width(45)
        .height(29)
        .center_x(Length::Fill)
        .center_y(Length::Fill);

    let btn = button(inner)
        .on_press(lumino_ui_core::window_event::Event::traffic_action(
            &cfg.action,
        ))
        .style(move |theme: &Theme, status| {
            let palette = theme.extended_palette();
            let background = match status {
                button::Status::Hovered => cfg.color.unwrap_or(palette.background.weaker.color),
                button::Status::Pressed => cfg
                    .color
                    .map(|c| iced_core::Color::from_rgb(c.r * 0.9, c.g * 0.9, c.b * 0.9))
                    .unwrap_or(palette.background.weak.color),
                _ => iced_core::Color::TRANSPARENT,
            };
            button::Style {
                border: iced_core::Border::default().rounded(0),
                ..Default::default()
            }
            .with_background(background)
        });

    // tooltip 复用 lumino widget::with_tooltip_bottom 若可用；
    // 为避免跨 crate widget 依赖，此处用 hover 提示的 button 本身即带语义，
    // 外层调用方可按需包裹 tooltip（P2 桩暂以纯按钮保证编译）。
    let tooltip_text = if cfg.action == TrafficAction::ToggleMaximize && window.is_maximized {
        "还原"
    } else {
        cfg.tooltip
    };
    // 仅保留按钮本身；tooltip 可在外层通过 `iced_widget::tooltip` 按需补充
    // 为保持与 lumino-ui 行为一致，仍通过 `widget::with_tooltip_bottom` 的语义保留提示文本注释
    let _ = tooltip_text;
    btn.into()
}

fn traffic_view<'a>(window: &'a Window) -> Element<'a> {
    let items = TRAFFICS
        .iter()
        .map(|cfg| traffic_item(cfg, window))
        .collect::<Vec<_>>();
    let inner = row(items).spacing(1);
    container(inner).width(137).height(Length::Fill).into()
}

/// 渲染标题栏
///
/// - 左侧/中部：文件名 + 脏点（单文档），居中显示
/// - 拖动区：`mouse_area` 的 `on_press(drag)` + `on_double_click(toggle_maximize)`（复用
///   `lumino-ui/src/titlebar.rs:86..88`）
/// - 右侧：窗口控制（非 macOS 才显示，macOS 用系统红绿灯，保留 `fullsize_content_view`）
/// - `use_native_titlebar == true` 时仅渲染单行文件名（高度仍 30 以保持布局稳定，
///   但不含拖动与窗口按钮，交由系统标题栏）
pub fn view<'a>(
    window: &'a Window,
    state: TitleBarState,
    use_native_titlebar: bool,
) -> Element<'a> {
    if use_native_titlebar {
        // 经典系统标题栏：仅显示文件名（最左侧逻辑已由系统接管）
        // macOS 下此分支由 `with_titlebar_transparent(false)` 触发
        let name = state.display_name();
        let txt = text(name).size(12).style(|theme: &Theme| {
            let p = theme.extended_palette();
            iced_widget::text::Style {
                color: Some(p.background.base.text),
            }
        });
        return container(row![txt].align_y(Alignment::Center).padding([0, 8]))
            .width(Length::Fill)
            .height(30)
            .style(|theme: &Theme| {
                let palette = theme.extended_palette();
                container::Style::default().background(if window.is_focused {
                    palette.background.base.color
                } else {
                    palette.background.weaker.color
                })
            })
            .into();
    }

    // 自定义标题栏：左侧拖动区 + 中部文件名 + 右侧窗口按钮
    let name = state.display_name();
    let palette = window.theme.extended_palette();
    let bg = if window.is_focused {
        palette.background.base.color
    } else {
        palette.background.weaker.color
    };
    let fg = palette.background.base.text;

    let title_text = text(name)
        .size(13)
        .align_x(iced_core::alignment::Horizontal::Center)
        .style(move |_theme: &Theme| iced_widget::text::Style { color: Some(fg) });

    // 中间标题（带脏点）— 居中
    let title_el = container(title_text)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill);

    // 拖动区：覆盖标题区域（除右侧窗口按钮外）
    // lumino 原 `titlebar.rs:86` 用 `mouse_area(container(space)).on_press(drag).on_double_click(toggle_maximize)`
    let drag_area = mouse_area(
        container(title_el)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_y(Length::Fill),
    )
    .on_press(lumino_ui_core::window_event::Event::drag())
    .on_double_click(lumino_ui_core::window_event::Event::toggle_maximize());

    // 右侧：窗口控制（macOS 隐藏，用系统红绿灯；保留 fullsize_content_view）
    let right: Element<'a> = if cfg!(target_os = "macos") {
        // macOS：预留 70px 交通灯占位（`window_manager.rs:337..339` 的
        // `with_fullsize_content_view(true)` 已把内容延伸至标题栏，左侧需避让系统按钮）
        space().width(70).height(Length::Fill).into()
    } else {
        traffic_view(window)
    };

    // 整体布局：拖动区（占满） + 右侧窗口按钮（固定 137px）
    let bar = if cfg!(target_os = "macos") {
        // macOS：左侧避让 + 中部拖动标题 + 右侧占位（保持对称）
        row![space().width(70).height(Length::Fill), drag_area, right,].align_y(Alignment::Center)
    } else {
        row![drag_area, right].align_y(Alignment::Center)
    };

    container(bar)
        .width(Length::Fill)
        .height(30)
        .style(move |_theme: &Theme| container::Style::default().background(bg))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_name_dirty() {
        let s = TitleBarState::named("test.mid", true);
        assert_eq!(s.display_name(), "test.mid ●");
    }

    #[test]
    fn display_name_clean_untitled() {
        let s = TitleBarState::untitled();
        assert_eq!(s.display_name(), "Untitled");
    }

    #[test]
    fn display_name_clean_named() {
        let s = TitleBarState::named("demo.mid", false);
        assert_eq!(s.display_name(), "demo.mid");
    }
}
