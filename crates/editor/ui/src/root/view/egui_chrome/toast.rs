//! 右下角浮层：Toast 通知 + 首次使用帮助提示（生产模型 + 动作）。
//!
//! 视图已迁到 egui（`lumino-ui-egui::chrome::toast`）；这里把生产状态镜像成
//! 纯数据，并把点击翻译回原有消息构造器。

use lumino_core::tr;
use lumino_ui_egui::chrome::{HelpTipModel, ToastCardModel, ToastModel};

use crate::Message;
use crate::root::Root;

/// 颜色 → egui 颜色（保留 alpha）。
fn color32(color: iced_core::Color) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(
        (color.r * 255.0).round() as u8,
        (color.g * 255.0).round() as u8,
        (color.b * 255.0).round() as u8,
        (color.a * 255.0).round() as u8,
    )
}

/// 本帧 Toast 卡片。
pub(crate) fn toast_model(root: &Root) -> ToastModel {
    ToastModel {
        cards: root
            .toast
            .egui_cards(&root.window.theme)
            .into_iter()
            .map(|(icon, message, bg, fg)| ToastCardModel {
                icon,
                message,
                background: color32(bg),
                text: color32(fg),
            })
            .collect(),
    }
}

/// 本帧帮助提示（无活动主题时为 `None`）。
pub(crate) fn help_tip_model(root: &Root) -> Option<HelpTipModel> {
    let language = root.settings.display.language;
    let topic = root.state.help_tips.active_topic()?;
    let body = topic
        .body_text(language)
        .split("\n\n")
        .next()
        .unwrap_or_default()
        .to_owned();
    Some(HelpTipModel {
        title: topic.title_text(language).to_owned(),
        body,
        help_label: tr!("查看帮助").to_owned(),
        dismiss_label: tr!("知道了").to_owned(),
    })
}

impl Root {
    /// 帮助提示动作 → 生产消息。
    pub(crate) fn help_tip_action_message(
        &self,
        action: lumino_ui_egui::chrome::toast::HelpTipAction,
    ) -> Message {
        use lumino_ui_egui::chrome::toast::HelpTipAction;
        match action {
            HelpTipAction::OpenHelp => Message::Core(crate::event::Event::Window(
                crate::event::window::Event::toggle_help_dialog(),
            )),
            HelpTipAction::Dismiss => Message::Help(lumino_message::HelpAction::DismissTip),
        }
    }
}

impl Root {
    /// egui 叠加层使用的 Toast 卡片（每帧装配）。
    pub(crate) fn egui_toast_model(&self) -> ToastModel {
        toast_model(self)
    }

    /// egui 叠加层使用的帮助提示。
    pub(crate) fn egui_help_tip_model(&self) -> Option<HelpTipModel> {
        help_tip_model(self)
    }
}
