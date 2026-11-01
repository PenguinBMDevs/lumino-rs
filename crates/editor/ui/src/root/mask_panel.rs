//! 遮罩参数面板（DRAW-010）
//!
//! 入口：选中一个待确认/已 Bake 形状 → 右键 →「遮罩设置…」。
//!
//! 写入时机（避免撤销栈被滑杆刷满）：
//! - **待确认形状**：即时写 `ShapeInstance.mask`，预览立刻变，不产生文档历史；
//! - **已 Bake 形状**：面板只改草稿，点 Apply 才 `set_mask`（一次 `edit_shape`）。

use iced_core::{Alignment, Background, Border, Color, Length, Padding, Point};
use iced_widget::{button, column, container, mouse_area, row, slider, text};
use lumino_core::shape::{MaskParams, MaskShow};
use lumino_message::MaskPanelAction;
use lumino_ui_editor::MaskEditTarget;
use lumino_ui_settings::components::with_setting_tooltip_inline;

use crate::{Element, Message};

/// 面板宽度
const PANEL_WIDTH: f32 = 250.0;
/// 锚点到鼠标点的偏移（避免压在右键位置）
const ANCHOR_OFFSET: f32 = 12.0;

/// 面板状态
#[derive(Debug, Default)]
pub(crate) struct MaskPanelState {
    /// 是否打开
    pub open: bool,
    /// 弹出锚点（窗口坐标）
    pub anchor: Point,
    /// 打开时锁定的编辑目标：选中变化不会把参数写到别的对象上
    pub target: Option<MaskEditTarget>,
    /// 面板当前参数（None = 未启用遮罩）
    pub params: Option<MaskParams>,
    /// 已 Bake 是否有未应用的草稿
    pub dirty: bool,
}

impl MaskPanelState {
    /// 打开面板：锁定目标，读取当前参数
    ///
    /// `params = None` 表示「该对象当前没有遮罩」——面板显示为未启用，
    /// 用户第一次改参数才会隐式启用（不要在打开时就假报 On）。
    pub fn show(&mut self, target: MaskEditTarget, params: Option<MaskParams>, anchor: Point) {
        self.open = true;
        self.target = Some(target);
        self.params = params;
        self.dirty = false;
        self.anchor = anchor;
    }

    /// 关闭面板并丢弃草稿
    pub fn hide(&mut self) {
        self.open = false;
        self.target = None;
        self.params = None;
        self.dirty = false;
    }

    /// 当前参数（未启用时返回默认值，供控件渲染）
    pub fn current(&self) -> MaskParams {
        self.params.clone().unwrap_or_default()
    }
}

/// 面板动作 → 消息
fn msg(action: MaskPanelAction) -> Message {
    Message::MaskPanel(action)
}

/// 渲染面板；未打开时返回 `None`
pub(crate) fn view_mask_panel(root: &crate::root::Root) -> Option<Element<'static>> {
    let state = &root.mask_panel;
    if !state.open {
        return None;
    }
    let params = state.current();
    let enabled = state.params.is_some();
    let baked = matches!(state.target, Some(MaskEditTarget::Baked { .. }));

    let theme = &root.window.theme;
    let panel_background = theme.extended_palette().background.weak.color;
    let border_color = theme.extended_palette().background.strong.color;
    let text_color = panel_text_color(panel_background);

    let title = text("遮罩").size(13).color(text_color);
    let close = button(text("✕").size(12))
        .padding(2)
        .style(transparent_button)
        .on_press(msg(MaskPanelAction::Close));
    let header = row![title, iced_widget::Space::new().width(Length::Fill), close]
        .align_y(Alignment::Center);

    let enable_row = row![
        label("启用", text_color),
        toggle("On", enabled, msg(MaskPanelAction::SetEnabled(!enabled))),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let show_row = row![
        label("区域", text_color),
        toggle(
            "内部",
            params.show == MaskShow::Inside,
            msg(MaskPanelAction::SetShow(MaskShow::Inside)),
        ),
        toggle(
            "沿线",
            params.show == MaskShow::Line,
            msg(MaskPanelAction::SetShow(MaskShow::Line)),
        ),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let size_row = row![
        label("带宽", text_color),
        slider(0.5..=16.0, params.size_keys, |v| msg(
            MaskPanelAction::SetSize(v)
        ))
        .step(0.5_f32)
        .width(Length::Fixed(110.0)),
        text(format!("{:.1}", params.size_keys))
            .size(11)
            .width(Length::Fixed(30.0))
            .color(text_color),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    let invert_row = row![
        label("取反", text_color),
        toggle(
            "挖洞",
            params.invert,
            msg(MaskPanelAction::SetInvert(!params.invert)),
        ),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let own_row = row![
        label("自身发声", text_color),
        toggle(
            "边框",
            params.own,
            msg(MaskPanelAction::SetOwn(!params.own)),
        ),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    let mut body = column![header, enable_row, show_row, size_row, invert_row, own_row]
        .spacing(6)
        .width(Length::Fixed(PANEL_WIDTH));

    if baked {
        let apply_label = if state.dirty { "应用 *" } else { "应用" };
        let apply_hint = "已 Bake 图形：点「应用」才写入文档（一次撤销单元）";
        body = body.push(
            row![
                with_setting_tooltip_inline(
                    text(if state.dirty {
                        "未应用"
                    } else {
                        "已同步"
                    })
                    .size(11)
                    .color(text_color),
                    apply_hint,
                ),
                iced_widget::Space::new().width(Length::Fill),
                button(text(apply_label).size(12))
                    .padding([3, 10])
                    .on_press(msg(MaskPanelAction::Apply)),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );
    }

    let panel = container(body)
        .padding(10)
        .width(Length::Fixed(PANEL_WIDTH + 20.0))
        .style(move |_theme: &iced_core::Theme| container::Style {
            background: Some(Background::Color(panel_background)),
            text_color: Some(text_color),
            border: Border {
                width: 1.0,
                color: border_color,
                radius: 8.0.into(),
            },
            ..Default::default()
        });

    // 外层：全屏容器 + 锚点内缩，点击面板外不拦截（与混音台同一套非阻塞覆盖层）。
    let x = state.anchor.x + ANCHOR_OFFSET;
    let y = state.anchor.y + ANCHOR_OFFSET;
    let outer = container(panel)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Start)
        .align_y(Alignment::Start)
        .padding(Padding {
            top: y.max(0.0),
            right: 0.0,
            bottom: 0.0,
            left: x.max(0.0),
        });
    // 点击面板之外的区域关闭（不吞滚动/其它交互时也能有一个明确出口）。
    let backdrop = mouse_area(
        iced_widget::Space::new()
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .on_press(msg(MaskPanelAction::Close));
    Some(
        iced_widget::Stack::new()
            .push(backdrop)
            .push(outer)
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
    )
}

fn label(value: &'static str, color: Color) -> Element<'static> {
    text(value)
        .size(12)
        .width(Length::Fixed(52.0))
        .color(color)
        .into()
}

fn toggle(value: &'static str, selected: bool, message: Message) -> Element<'static> {
    let background = selected.then(|| Background::Color(Color::from_rgba(0.22, 0.42, 0.62, 0.9)));
    button(text(value).size(11))
        .on_press(message)
        .padding([2, 8])
        .style(move |_theme: &iced_core::Theme, _status| button::Style {
            background,
            border: Border::default().rounded(4),
            ..Default::default()
        })
        .into()
}

fn transparent_button(theme: &iced_core::Theme, _status: button::Status) -> button::Style {
    button::Style {
        text_color: theme.extended_palette().background.base.text,
        ..Default::default()
    }
}

/// 面板底色上的可读文字色（与文字参数面板同一口径）
fn panel_text_color(background: Color) -> Color {
    let luminance = 0.2126 * background.r + 0.7152 * background.g + 0.0722 * background.b;
    if luminance > 0.55 {
        Color::from_rgba(0.08, 0.08, 0.10, 1.0)
    } else {
        Color::from_rgba(0.95, 0.95, 0.95, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_show_hide_state() {
        let mut state = MaskPanelState::default();
        assert!(!state.open);
        state.show(
            MaskEditTarget::Pending { index: 0 },
            None,
            Point::new(10.0, 20.0),
        );
        assert!(state.open);
        assert_eq!(state.target, Some(MaskEditTarget::Pending { index: 0 }));
        assert!(state.params.is_none(), "未启用时不能假报 On");
        assert_eq!(
            state.current().size_keys,
            MaskParams::default().size_keys,
            "控件仍应拿到默认值渲染"
        );
        state.hide();
        assert!(!state.open);
        assert!(state.target.is_none());
    }

    #[test]
    fn test_text_color_contrasts_with_background() {
        let dark = panel_text_color(Color::from_rgba(0.1, 0.1, 0.1, 1.0));
        let light = panel_text_color(Color::from_rgba(0.9, 0.9, 0.9, 1.0));
        assert!(dark.r > 0.5);
        assert!(light.r < 0.5);
    }
}
