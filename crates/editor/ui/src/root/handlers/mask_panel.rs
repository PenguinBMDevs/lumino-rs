//! 遮罩参数面板处理器（DRAW-010）
//!
//! DEC-001：Bake 后形状对象只读，面板只对 **✓ 前的待确认对象** 开放，
//! 参数即时写 pending（不产生文档历史）。

use lumino_core::tr;
use lumino_message::MaskPanelAction;
use lumino_ui_editor::MaskEditTarget;

use crate::root::Root;
use crate::toast::ToastLevel;

impl Root {
    /// 打开面板：锁定当前选中的遮罩编辑目标
    ///
    /// `anchor` 为窗口坐标（右键位置换算后）。
    pub(crate) fn open_mask_panel(&mut self, anchor: iced_core::Point) {
        let Some(target) = self.editor.mask_edit_target() else {
            self.toast.push(
                ToastLevel::Warning,
                tr!("遮罩设置失败：请选中一个待确认形状"),
            );
            return;
        };
        let params = self.editor.mask_params_of(target);
        self.mask_panel.show(target, params, anchor);
    }

    /// 处理遮罩面板动作
    pub(crate) fn handle_mask_panel(&mut self, action: MaskPanelAction) {
        if matches!(action, MaskPanelAction::Close) {
            self.mask_panel.hide();
            return;
        }
        if !self.mask_panel.open {
            return;
        }
        if matches!(action, MaskPanelAction::Apply) {
            return;
        }
        if matches!(action, MaskPanelAction::ApplyAsShape) {
            return;
        }

        match action {
            MaskPanelAction::SetEnabled(false) => {
                self.mask_panel.params = None;
                self.push_mask_params();
            }
            MaskPanelAction::SetEnabled(true) => {
                if self.mask_panel.params.is_none() {
                    // 重新启用：沿用默认参数，不沿用「关掉前」的旧值，
                    // 避免用户以为关掉=取消却还残留着上一次的挖洞/取反。
                    self.mask_panel.params = Some(lumino_core::shape::MaskParams::default());
                }
                self.push_mask_params();
            }
            MaskPanelAction::SetSize(size) => {
                let mut params = self.mask_panel.current();
                params.size_keys = size.clamp(0.5, 32.0);
                self.mask_panel.params = Some(params);
                self.push_mask_params();
            }
            MaskPanelAction::SetInvert(invert) => {
                let mut params = self.mask_panel.current();
                params.invert = invert;
                self.mask_panel.params = Some(params);
                self.push_mask_params();
            }
            MaskPanelAction::SetOwn(own) => {
                let mut params = self.mask_panel.current();
                params.own = own;
                self.mask_panel.params = Some(params);
                self.push_mask_params();
            }
            MaskPanelAction::SetShow(show) => {
                let mut params = self.mask_panel.current();
                params.show = show;
                self.mask_panel.params = Some(params);
                self.push_mask_params();
            }
            MaskPanelAction::Close | MaskPanelAction::Apply | MaskPanelAction::ApplyAsShape => {}
        }
    }

    /// 参数 → 目标：只有待确认对象，参数即时生效
    fn push_mask_params(&mut self) {
        let Some(target) = self.mask_panel.target else {
            return;
        };
        match target {
            MaskEditTarget::Pending { index } => {
                self.editor
                    .set_pending_mask(index, self.mask_panel.params.clone());
            }
            MaskEditTarget::PendingComposed { index } => {
                self.editor
                    .set_pending_composed_mask(index, self.mask_panel.params.clone());
            }
        }
    }
}
