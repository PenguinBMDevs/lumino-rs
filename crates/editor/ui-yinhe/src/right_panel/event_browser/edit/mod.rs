//! 右键编辑 popup — 对应 yinhe `right_panel/event_browser/edit.rs:364`
//!
//! 设计（pending + 关闭时一次性 apply，iced 侧以 `Message` 单向流重构）：
//! - yinhe 原：cell 右键写 `egui::Id::new((salt, "edit"))` 的 `EditRequest`，
//!   `apply_*_popups` 每帧 `peek_edit_request` 显示 `Area + DragValue + confirm/cancel`，
//!   关闭时一次性 `apply + push_undo`。
//! - iced 桩：无 `egui::Area` / `Id::new((salt, "edit"))`，改为 `column + row` 的
//!   内联编辑区占位；`PopupAction` 保留 `None / Closed(v) / Cancelled` 三态，
//!   待接入 `Message` 后由 Host 统一 `apply + push_undo`。
//!
//! 按事件类型拆分为子模块（与 yinhe 一一对应）：
//! - `automation` — CC/PB/RPN/NRPN/Tempo 的 value/tick/shape
//! - `note` — 音符的 start_tick/end_tick/gate/key/velocity
//! - `timesig` — 拍号的 tick/numerator/denominator
//! - `keysig` — 调号的 tick/root/scale
//! - `pc` — Program Change 的 tick/program
//! - `text` — Marker/Lyrics/Chord 的 tick/text

pub mod automation;
pub mod keysig;
pub mod note;
pub mod pc;
pub mod text;
pub mod timesig;

pub use automation::view as automation_view;
pub use keysig::view as keysig_view;
pub use note::view as note_view;
pub use pc::view as pc_view;
pub use text::view as text_view;
pub use timesig::view as timesig_view;

use iced_widget::{button, column, container, row, text as widget_text};

use lumino_ui_core::{Element, Theme, window::Window};

/// popup 关闭事件（对齐 yinhe `PopupAction`）
///
/// `None` — 仍打开；`Closed(v)` — 确认并携带最终值；`Cancelled` — 取消。
#[derive(Debug, Clone, PartialEq)]
pub enum PopupAction {
    None,
    Closed(f64),
    Cancelled,
}

/// 下拉选择 popup 关闭事件（对齐 yinhe `ChoicePopupAction`）
#[derive(Debug, Clone, PartialEq)]
pub enum ChoicePopupAction<T> {
    None,
    Closed(T),
    Cancelled,
}

/// 数字编辑 popup 占位（`Area + DragValue + confirm/cancel` 的 iced 等价）
///
/// 以 `column` 占位标题 + 数值 + 确认/取消按钮，样式走 `Theme`。
pub fn number_popup_view<'a>(
    window: &'a Window,
    title: String,
    value: f64,
    range: (f64, f64),
) -> Element<'a> {
    let palette = window.theme.extended_palette();
    container(
        column![
            widget_text(title).size(11).style(move |_theme: &Theme| {
                iced_widget::text::Style {
                    color: Some(palette.background.strong.text),
                }
            }),
            widget_text(format!("{value:.2}  (range {range:?})")).size(11),
            row![
                button(widget_text("Confirm").size(11)).padding([4, 8]),
                button(widget_text("Cancel").size(11)).padding([4, 8]),
            ]
            .spacing(8),
        ]
        .spacing(6)
        .padding([8, 8]),
    )
    .style(move |_theme: &Theme| container::Style {
        background: Some(iced_core::Background::Color(palette.background.base.color)),
        border: iced_core::Border {
            color: palette.background.strong.color,
            width: 1.0,
            radius: 6.0.into(),
        },
        ..Default::default()
    })
    .into()
}

/// tick 位置编辑 popup 占位（小节 + 小节内 tick 双输入，`BarLookup` 语义保留）
pub fn position_popup_view<'a>(
    window: &'a Window,
    title: String,
    tick: u32,
    bar: u32,
    tick_in_bar: u32,
) -> Element<'a> {
    container(
        column![
            widget_text(title).size(11),
            row![
                widget_text("Bar:").size(11),
                widget_text(bar.to_string()).size(11),
                widget_text("/").size(11),
                widget_text(tick_in_bar.to_string()).size(11),
                widget_text(format!("(tick {tick})")).size(10),
            ]
            .spacing(6),
            row![
                button(widget_text("Confirm").size(11)).padding([4, 8]),
                button(widget_text("Cancel").size(11)).padding([4, 8]),
            ]
            .spacing(8),
        ]
        .spacing(6)
        .padding([8, 8]),
    )
    .style(move |_theme: &Theme| container::Style {
        background: Some(iced_core::Background::Color(
            window.theme.extended_palette().background.base.color,
        )),
        ..Default::default()
    })
    .into()
}

/// 供各子模块复用的 `EventList` undo 占位（对齐 yinhe `push_event_list_undo`）
///
/// iced 桩不直接持有 `Document`，仅保留签名约束：before != after 时才 push。
#[must_use]
pub fn should_push_event_list_undo<T: PartialEq>(before: &[T], after: &[T]) -> bool {
    before != after
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_ui_core::window::Window;

    #[test]
    fn number_popup_does_not_panic() {
        let window = Window::new("Tokyo Night Storm");
        let _el = number_popup_view(&window, "Edit tick".to_string(), 480.0, (0.0, 1_000_000.0));
    }

    #[test]
    fn position_popup_does_not_panic() {
        let window = Window::new("Tokyo Night Storm");
        let _el = position_popup_view(&window, "Edit position".to_string(), 480, 1, 480);
    }
}
