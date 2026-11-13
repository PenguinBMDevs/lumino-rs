//! 「关于」页 logo 彩蛋飞行层 → egui 模型。
//!
//! 状态机（`ui-core::state::about_egg`）输出位置 / 旋转角 / 透明度；这里原样
//! 搬进 [`EggModel`]，由 `ui-egui::chrome::egg` 用旋转网格绘制。

use lumino_ui_core::state::about_egg::{ABOUT_LOGO_BOX, ABOUT_LOGO_SIZE};
use lumino_ui_egui::chrome::EggModel;

use crate::root::Root;

impl Root {
    /// 飞行中的 logo 快照；不在飞行中返回 `None`。
    pub(crate) fn egui_egg_model(&self) -> Option<EggModel> {
        let egg = &self.state.about_egg;
        if !egg.is_airborne() {
            return None;
        }
        let (x, y) = egg.position();
        Some(EggModel {
            x,
            y,
            box_size: ABOUT_LOGO_BOX,
            icon_size: ABOUT_LOGO_SIZE * egg.spin_fit_scale(),
            angle: egg.spin_angle(),
            opacity: egg.opacity(),
        })
    }
}
