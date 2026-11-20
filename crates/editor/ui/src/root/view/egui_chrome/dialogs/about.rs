//! 关于对话框：品牌行与版本号 → egui 模型。
//!
//! 版本号取编译期 `CARGO_PKG_VERSION`，避免与 i18n 表里的手写版本号漂移。

use lumino_core::{tr, trf};
use lumino_ui_egui::chrome::AboutModel;

impl crate::root::Root {
    /// 关于对话框模型：标题行 + 版本行（均按当前语言取词）。
    pub(super) fn about_dialog_model(&self) -> AboutModel {
        AboutModel {
            titlebar: !self.settings.synth.use_native_titlebar,
            title: tr!("Lumino - 新一代MIDI编辑器").to_string(),
            version: trf!("版本 {version}", version = crate::APP_VERSION),
        }
    }
}
