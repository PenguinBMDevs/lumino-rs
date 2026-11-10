//! 遮罩参数面板（DRAW-010）
//!
//! 入口：选中一个待确认/已 Bake 形状 → 右键 →「遮罩设置…」。
//!
//! 写入时机（避免撤销栈被滑杆刷满）：
//! - **待确认形状**：即时写 `ShapeInstance.mask`，预览立刻变，不产生文档历史；
//! - **已 Bake 形状**：面板只改草稿，点 Apply 才 `set_mask`（一次 `edit_shape`）。
//!
//! 视图已迁到 egui（`lumino_ui_egui::mask_panel` + `egui_chrome::mask_panel`）：
//! 本文件只保留 **UI 无关状态**，不再有 iced 视图。

use iced_core::Point;
use lumino_core::shape::MaskParams;
use lumino_ui_editor::MaskEditTarget;

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
}
