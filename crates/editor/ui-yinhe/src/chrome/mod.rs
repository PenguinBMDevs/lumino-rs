//! Chrome — Yinhe 标题/传输/模式栏的 iced 聚合层
//!
//! 对应 `yinhe/crates/yinhe-egui/src/chrome` 下 `title_bar.rs:520` +
//! `transport_bar.rs:1912` + `mode_bar.rs:324` 在 lumino 侧的 iced 桩。
//!
//! 设计约束（P2）：
//! - 不引入 `egui/eframe`，仅 `iced_widget` + `lumino-ui-core`（主题/字体/SVG 图标）
//! - 多标签 → 单文档（文件名 + 脏点 ●）
//! - 复用 `lumino-ui/src/titlebar/traffic.rs` 的窗口控制与 `toolbar/buttons.rs` 的按钮风格
//! - `Message` 统一走 `lumino_message`（经 `lumino-ui-core::Message` 实例化）
//! - `macOS` 保留 `with_titlebar_transparent + with_fullsize_content_view`
//!  （`src/runner/window_manager.rs:337..339`），视图侧隐藏自绘红绿灯

pub mod mode_bar;
pub mod title_bar;
pub mod transport_bar;

pub use mode_bar::{ModeBarMetrics, ViewMode};
pub use title_bar::TitleBarState;
pub use transport_bar::TransportState;

use iced_widget::{column, container};
use lumino_ui_core::app_mode::AppMode;
use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

/// 顶层 chrome 状态（聚合三栏所需状态，供上层 `Host/Root` 持有）
///
/// P2 仅聚合最小可编译子集；后续 P3 可在此追加 `RightTab`、轨道/工程等全量状态。
#[derive(Debug, Clone)]
pub struct ChromeState {
    /// 当前 Yinhe 视图模式
    pub view_mode: ViewMode,
    /// Arrange 模式下是否叠加钢琴卷帘（yinhe `show_pianoroll_in_arrange`）
    pub show_pianoroll_in_arrange: bool,
    /// 标题栏（文件名 + 脏点）
    pub title: TitleBarState,
    /// 传输栏
    pub transport: TransportState,
    /// 模式栏右侧指标（可选）
    pub mode_metrics: Option<ModeBarMetrics>,
    /// 是否使用系统标题栏（`use_native_titlebar`）
    pub use_native_titlebar: bool,
}

impl Default for ChromeState {
    fn default() -> Self {
        Self {
            view_mode: ViewMode::Arrange,
            show_pianoroll_in_arrange: false,
            title: TitleBarState::untitled(),
            transport: TransportState::default(),
            mode_metrics: None,
            use_native_titlebar: false,
        }
    }
}

/// 渲染完整 chrome（标题 + 传输 + 模式栏）
///
/// 导出 `pub fn view(...) -> Element` 满足任务约束；
//
//  布局：
///
/// ```text
/// ┌─ title_bar（30px，拖动 + 文件名● + 交通灯）─┐
/// ├─ transport_bar（40px，播放/录音/速度/拍号/量化）┤
/// └─ mode_bar（28px，ARRANGE/PIANO/MIX + 指标）─────┘
/// ```
///
/// `window` 提供主题与窗口状态；`app_mode` 用于高亮 Yinhe 分支；
///
/// `chrome_state` 聚合三栏业务状态。
pub fn view<'a>(
    window: &'a Window,
    app_mode: AppMode,
    chrome_state: ChromeState,
) -> Element<'a> {
    let title_el = title_bar::view(
        window,
        chrome_state.title.clone(),
        chrome_state.use_native_titlebar,
    );
    let transport_el = transport_bar::view(window, chrome_state.transport.clone());
    let mode_el = mode_bar::view(
        window,
        app_mode,
        chrome_state.view_mode,
        chrome_state.show_pianoroll_in_arrange,
        chrome_state.mode_metrics,
    );

    // 外层容器：三栏纵向叠加，背景随主题（与 title_bar 保持一致）
    let palette = window.theme.extended_palette();
    let bg = palette.background.base.color;

    container(column![title_el, transport_el, mode_el].spacing(0))
        .width(iced_core::Length::Fill)
        .style(move |_theme: &Theme| container::Style {
            background: Some(iced_core::Background::Color(bg)),
            ..Default::default()
        })
        .into()
}

/// 便捷重导出：供测试/文档示例直接构造最小可视 chrome
///
/// ```ignore
/// use lumino_ui_yinhe::chrome::{view, ChromeState, ViewMode};
/// use lumino_ui_core::{window::Window, app_mode::AppMode};
/// let window = Window::new("Tokyo Night Storm");
/// let state = ChromeState::default();
/// let el = view(&window, AppMode::Yinhe, &state);
/// ```
#[cfg(test)]
mod tests {
    use super::*;
    use lumino_ui_core::window::Window;

    #[test]
    fn chrome_view_does_not_panic() {
        let window = Window::new("Tokyo Night Storm");
        let state = ChromeState {
            title: TitleBarState::named("demo.mid", true),
            transport: TransportState {
                is_playing: true,
                bpm: 140.0,
                has_active_document: true,
                ..Default::default()
            },
            view_mode: ViewMode::Piano,
            mode_metrics: Some(ModeBarMetrics {
                cpu_usage: 12.3,
                mem_mb: 256.0,
                fps: 60.0,
            }),
            ..Default::default()
        };
        // 仅断言 view 可构造 Element 且不 panic（不实际渲染到 wgpu）
        let _el = view(&window, AppMode::Yinhe, &state);
    }

    #[test]
    fn chrome_view_native_titlebar() {
        let window = Window::new("Tokyo Night Storm");
        let mut state = ChromeState::default();
        state.use_native_titlebar = true;
        let _el = view(&window, AppMode::Editor, &state);
    }
}
