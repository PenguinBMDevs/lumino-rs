//! 混音台独立窗口的本地状态与装配方法。
//!
//! 对话框是独立 Root：主窗口的轨道 / 电平快照由 Runner 每帧推入
//! [`MixerDialogState::model`]；横向滚动是**本地**状态（主窗口不关心），
//! 动作先累积到 [`MixerDialogState::actions`]，由 Runner 取走并回放到主窗口。

use lumino_ui_egui::mixer::{MixerAction, MixerModel};

/// 混音台对话框状态。
#[derive(Debug, Clone, Default)]
pub(crate) struct MixerDialogState {
    /// 主窗口推送的通道条 / 电平快照。
    pub(crate) model: MixerModel,
    /// 本地横向滚动偏移（逻辑像素）。
    pub(crate) scroll_x: f32,
    /// 本帧待回放到主窗口的动作（`Scrolled` 不进入此队列）。
    pub(crate) actions: Vec<MixerAction>,
}

impl crate::root::Root {
    /// 主窗口是否请求打开混音台独立窗口。
    pub(crate) fn mixer_open(&self) -> bool {
        self.mixer_panel.open
    }

    /// 设置“是否请求打开混音台独立窗口”的镜像开关。
    ///
    /// 只改主窗口镜像；真正的窗口开关由 Runner 每帧对账。
    pub(crate) fn set_mixer_open(&mut self, open: bool) {
        self.mixer_panel.open = open;
    }

    /// 注入主窗口推送的混音台快照；返回是否有变化。
    ///
    /// 比较的是快照本身（不含本地滚动），因此滚动不会触发无意义的置脏。
    pub(crate) fn set_mixer_dialog_model(&mut self, model: MixerModel) -> bool {
        if self.mixer_dialog.model == model {
            return false;
        }
        self.mixer_dialog.model = model;
        true
    }

    /// 取走对话框内产生的动作（供 Runner 回放到主窗口）。
    pub(crate) fn take_mixer_dialog_actions(&mut self) -> Vec<MixerAction> {
        std::mem::take(&mut self.mixer_dialog.actions)
    }

    /// 处理对话框内产生的单条动作。
    ///
    /// 滚动是本地状态，立即生效；其余动作排队，等 Runner 统一回放。
    pub(crate) fn apply_mixer_dialog_action(&mut self, action: MixerAction) {
        match action {
            MixerAction::Scrolled(x) => self.mixer_dialog.scroll_x = x.max(0.0),
            other => self.mixer_dialog.actions.push(other),
        }
    }
}
