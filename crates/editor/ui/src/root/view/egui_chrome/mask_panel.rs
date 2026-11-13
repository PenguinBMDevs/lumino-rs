//! 遮罩参数面板：生产状态 → egui 模型 + 动作回传。
//!
//! 面板锚点为 `MaskPanelState.anchor`（全局鼠标点），原点再右下偏移 12——
//! 与 旧版 原实现同口径。

use lumino_core::shape::MaskShow;
use lumino_message::MaskPanelAction as MsgAction;
use lumino_ui_egui::mask_panel::{MaskPanelAction, MaskPanelLabels, MaskPanelModel, MaskShowKind};

use crate::root::Root;

impl Root {
    /// 遮罩面板模型；未打开时返回 `None`。
    pub(crate) fn mask_panel_model(&self) -> Option<MaskPanelModel> {
        let state = &self.mask_panel;
        if !state.open {
            return None;
        }
        let params = state.current();
        Some(MaskPanelModel {
            origin: egui::pos2(
                state.anchor.x + ANCHOR_OFFSET,
                state.anchor.y + ANCHOR_OFFSET,
            ),
            enabled: state.params.is_some(),
            show: match params.show {
                MaskShow::Inside => MaskShowKind::Inside,
                MaskShow::Line => MaskShowKind::Line,
            },
            size_keys: params.size_keys,
            invert: params.invert,
            own: params.own,
            // DEC-001：Bake 后形状只读，面板只对 pending 开放，不再有「应用」行。
            baked: false,
            dirty: state.dirty,
            labels: labels(),
        })
    }

    /// 动作 → 生产消息。
    pub(crate) fn mask_panel_action_message(&self, action: MaskPanelAction) -> crate::Message {
        let message = match action {
            MaskPanelAction::Close => MsgAction::Close,
            MaskPanelAction::SetEnabled(enabled) => MsgAction::SetEnabled(enabled),
            MaskPanelAction::SetShow(kind) => MsgAction::SetShow(match kind {
                MaskShowKind::Inside => MaskShow::Inside,
                MaskShowKind::Line => MaskShow::Line,
            }),
            MaskPanelAction::SetSize(size) => MsgAction::SetSize(size),
            MaskPanelAction::SetInvert(invert) => MsgAction::SetInvert(invert),
            MaskPanelAction::SetOwn(own) => MsgAction::SetOwn(own),
            MaskPanelAction::Apply => MsgAction::Apply,
        };
        crate::Message::MaskPanel(message)
    }
}

/// 锚点到面板原点的偏移（与 旧版 `ANCHOR_OFFSET` 同为 12）。
const ANCHOR_OFFSET: f32 = 12.0;

/// 面板文案（对应 旧版 视图里的字面量）。
fn labels() -> MaskPanelLabels {
    MaskPanelLabels {
        title: "遮罩".to_string(),
        close: "×".to_string(),
        enable: "启用".to_string(),
        enable_on: "On".to_string(),
        region: "区域".to_string(),
        inside: "内部".to_string(),
        line: "沿线".to_string(),
        size: "带宽".to_string(),
        invert: "取反".to_string(),
        hole: "挖洞".to_string(),
        own: "自身发声".to_string(),
        own_border: "边框".to_string(),
        apply: "应用".to_string(),
        apply_dirty: "应用 *".to_string(),
        synced: "已同步".to_string(),
        pending: "未应用".to_string(),
        apply_hint: "已 Bake 图形：点「应用」才写入文档（一次撤销单元）".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::root_state::DialogType;
    use lumino_core::shape::MaskParams;

    fn root() -> Root {
        Root::new_dialog(
            &lumino_ui_core::Theme::TokyoNightStorm.to_string(),
            DialogType::VideoExport,
        )
    }

    #[test]
    fn closed_panel_has_no_model() {
        assert!(root().mask_panel_model().is_none());
    }

    #[test]
    fn open_panel_mirrors_state() {
        let mut root = root();
        root.mask_panel.show(
            lumino_ui_editor::MaskEditTarget::Pending { index: 0 },
            Some(MaskParams {
                show: MaskShow::Line,
                size_keys: 6.5,
                invert: true,
                own: true,
                ..MaskParams::default()
            }),
            lumino_ui_core::geom::Point::new(100.0, 200.0),
        );
        let model = root.mask_panel_model().expect("打开时应有模型");
        assert_eq!(model.origin, egui::pos2(112.0, 212.0));
        assert!(model.enabled);
        assert_eq!(model.show, MaskShowKind::Line);
        assert_eq!(model.size_keys, 6.5);
        assert!(model.invert && model.own);
        assert!(!model.baked, "pending 目标不显示应用行");
    }

    #[test]
    fn actions_map_to_mask_panel_messages() {
        let root = root();
        assert!(matches!(
            root.mask_panel_action_message(MaskPanelAction::SetShow(MaskShowKind::Line)),
            crate::Message::MaskPanel(MsgAction::SetShow(MaskShow::Line))
        ));
        assert!(matches!(
            root.mask_panel_action_message(MaskPanelAction::SetSize(3.5)),
            crate::Message::MaskPanel(MsgAction::SetSize(v)) if (v - 3.5).abs() < f32::EPSILON
        ));
        assert!(matches!(
            root.mask_panel_action_message(MaskPanelAction::Apply),
            crate::Message::MaskPanel(MsgAction::Apply)
        ));
        assert!(matches!(
            root.mask_panel_action_message(MaskPanelAction::Close),
            crate::Message::MaskPanel(MsgAction::Close)
        ));
    }
}
