//! 工具栏弹层：模型构建 + 动作解析。
//!
//! `lumino-ui-egui` 只认识纯数据（`SnapMenuModel` / `OnionMenuModel`）与
//! `ToolbarAction`；真正的事件全部在这里翻译成生产 `Message`，
//! 与 iced 工具栏按钮走同一条 `toolbar::Event` 构造器。

use lumino_core::SnapSpec;
use lumino_core::tr;
use lumino_ui_core::onion_skin_mode::OnionSkinMode;
use lumino_ui_egui::chrome::{
    OnionMenuItem, OnionMenuModel, SnapMenuItem, SnapMenuModel, ToolbarAction,
};

use crate::Message;
use crate::message::CustomPrecisionAction;
use crate::root::Root;
use crate::toolbar;

/// 精度下拉是否打开 → egui 模型（关闭时为 `None`）。
pub(super) fn snap_menu(toolbar: &toolbar::Toolbar) -> Option<SnapMenuModel> {
    toolbar.snap_picker_open.then(|| SnapMenuModel {
        items: SnapSpec::presets()
            .into_iter()
            .map(|spec| SnapMenuItem {
                spec,
                label: spec.text(),
                selected: spec == toolbar.snap_spec,
            })
            .collect(),
        // 与 iced 视图逐字一致（该行在 iced 侧也没有本地化）。
        custom_label: "Custom…".to_owned(),
    })
}

/// 洋葱皮范围菜单是否打开 → egui 模型。
pub(super) fn onion_menu(toolbar: &toolbar::Toolbar) -> Option<OnionMenuModel> {
    toolbar.onion_skin_menu_open.then(|| OnionMenuModel {
        items: OnionSkinMode::all()
            .into_iter()
            .map(|mode| OnionMenuItem {
                label: onion_label(mode).to_owned(),
                selected: mode == toolbar.onion_skin_mode,
            })
            .collect(),
    })
}

/// 与 iced `render_onion_skin_button` 同口径的本地化标签。
fn onion_label(mode: OnionSkinMode) -> &'static str {
    match mode {
        OnionSkinMode::AllTracks => tr!("显示全部音轨"),
        OnionSkinMode::PreviousTrack => tr!("显示上一个音轨"),
        OnionSkinMode::NextTrack => tr!("显示下一个音轨"),
        OnionSkinMode::SpecificTrack => tr!("显示指定音轨…"),
    }
}

impl Root {
    /// 工具栏弹层动作 → 生产 `Message`。
    pub(crate) fn toolbar_action_message(&self, action: ToolbarAction) -> Option<Message> {
        Some(match action {
            ToolbarAction::ToggleSnapPicker => toolbar::Event::toggle_snap_picker(),
            ToolbarAction::SnapSpec(spec) => toolbar::Event::snap_spec_changed(spec),
            ToolbarAction::SnapCustom => Message::Batch(vec![
                Message::CustomPrecision(CustomPrecisionAction::OpenDialog),
                toolbar::Event::close_snap_picker(),
            ]),
            ToolbarAction::CloseSnapPicker => toolbar::Event::close_snap_picker(),
            ToolbarAction::ToggleOnionSkin => toolbar::Event::toggle_onion_skin(),
            ToolbarAction::ToggleOnionMenu => toolbar::Event::toggle_onion_skin_menu(),
            ToolbarAction::OnionMode(index) => {
                let mode = *OnionSkinMode::all().get(index)?;
                toolbar::Event::onion_skin_mode_selected(mode)
            }
            ToolbarAction::CloseOnionMenu => toolbar::Event::close_onion_skin_menu(),
        })
    }
}
