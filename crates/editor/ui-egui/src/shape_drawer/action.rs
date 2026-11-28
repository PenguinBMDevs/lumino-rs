//! 形状抽屉的动作与单帧通道。

/// 面板产生的动作（统一经此通道回传）。
#[derive(Debug, Clone, PartialEq)]
pub enum ShapeDrawerAction {
    /// 面板按钮 / 库条目动作。
    Panel(crate::chrome::ShapePanelAction),
}

const ACTION_ID: &str = "lumino.shape-drawer.action";

/// 记录本帧动作（一帧只保留第一个）。
pub fn push_action(ctx: &egui::Context, action: ShapeDrawerAction) {
    crate::action_slot::Slot::new(egui::Id::unique(ACTION_ID)).push_first(ctx, action);
}

/// 取走本帧动作。
pub fn take_action(ctx: &egui::Context) -> Option<ShapeDrawerAction> {
    crate::action_slot::Slot::new(egui::Id::unique(ACTION_ID)).take(ctx)
}
