//! 遮罩参数面板处理器（DRAW-010）
//!
//! 写入策略见 `crate::root::mask_panel`：
//! pending 即时写（不进文档历史），已 Bake 走草稿 + Apply（一次撤销单元）。

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
                tr!("遮罩设置失败：请选中一个待确认形状或已 Bake 图形"),
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
            self.apply_mask_panel();
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
            MaskPanelAction::Close | MaskPanelAction::Apply => {}
        }
    }

    /// 参数 → 目标：pending 即时生效；已 Bake 只标脏，等 Apply
    fn push_mask_params(&mut self) {
        let Some(target) = self.mask_panel.target else {
            return;
        };
        match target {
            MaskEditTarget::Pending { index } => {
                self.editor
                    .set_pending_mask(index, self.mask_panel.params.clone());
                self.mask_panel.dirty = false;
            }
            MaskEditTarget::PendingComposed { index } => {
                self.editor
                    .set_pending_composed_mask(index, self.mask_panel.params.clone());
                self.mask_panel.dirty = false;
            }
            MaskEditTarget::Baked { .. } => {
                self.mask_panel.dirty = true;
            }
        }
    }

    /// 已 Bake：把草稿写入文档（一次 `edit_shape` 撤销单元）
    fn apply_mask_panel(&mut self) {
        let Some(MaskEditTarget::Baked { model_id }) = self.mask_panel.target else {
            return;
        };
        let params = self.mask_panel.params.clone();
        let ok = self.editor.set_mask(model_id, params);
        if ok {
            self.mask_panel.dirty = false;
            self.toast.push(ToastLevel::Info, tr!("已应用遮罩"));
        } else {
            self.toast
                .push(ToastLevel::Warning, tr!("应用遮罩失败：目标图形已不可编辑"));
        }
    }
}
