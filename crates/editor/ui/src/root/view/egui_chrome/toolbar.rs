//! 工具栏弹层：模型构建 + 动作解析。
//!
//! `lumino-ui-egui` 只认识纯数据（`SnapMenuModel` / `OnionMenuModel`）与
//! `ToolbarAction`；真正的事件全部在这里翻译成生产 `Message`，
//! 与 iced 工具栏按钮走同一条 `toolbar::Event` 构造器。

use lumino_core::SnapSpec;
use lumino_core::tr;
use lumino_ui_core::onion_skin_mode::OnionSkinMode;
use lumino_ui_egui::chrome::{
    OnionMenuItem, OnionMenuModel, OverflowItemModel, OverflowMenuModel, SnapMenuItem,
    SnapMenuModel, ToolbarAction, ToolbarGroupsModel,
};
use lumino_ui_egui::icons::Icon;

use crate::Message;
use crate::message::CustomPrecisionAction;
use crate::root::Root;
use crate::toolbar;
use crate::toolbar::overflow::{ToolbarGroup, ToolbarOverflow};

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
    pub(crate) fn toolbar_action_message(&self, action: &ToolbarAction) -> Option<Message> {
        Some(match action {
            ToolbarAction::ToggleSnapPicker => toolbar::Event::toggle_snap_picker(),
            ToolbarAction::SnapSpec(spec) => toolbar::Event::snap_spec_changed(*spec),
            ToolbarAction::SnapCustom => Message::Batch(vec![
                Message::CustomPrecision(CustomPrecisionAction::OpenDialog),
                toolbar::Event::close_snap_picker(),
            ]),
            ToolbarAction::CloseSnapPicker => toolbar::Event::close_snap_picker(),
            ToolbarAction::ToggleOnionSkin => toolbar::Event::toggle_onion_skin(),
            ToolbarAction::ToggleOnionMenu => toolbar::Event::toggle_onion_skin_menu(),
            ToolbarAction::OnionMode(index) => {
                let mode = *OnionSkinMode::all().get(*index)?;
                toolbar::Event::onion_skin_mode_selected(mode)
            }
            ToolbarAction::CloseOnionMenu => toolbar::Event::close_onion_skin_menu(),
            ToolbarAction::ToggleOverflowMenu => toolbar::Event::toggle_overflow_menu(),
            ToolbarAction::CloseOverflowMenu => toolbar::Event::close_overflow_menu(),
            // 溢出项按「同宽度重算折叠结果」解析，需要视口宽度，见
            // [`Root::overflow_action_message`]（Host 单独调用）。
            ToolbarAction::OverflowItem(_) => return None,
            // 悬浮条及其锚定面板的动作在独立模块里翻译（保持单文件行数纪律）。
            other => return super::tool_panel::action_message(self, other),
        })
    }

    /// 工具栏溢出菜单项 → 生产 `Message`。
    ///
    /// 下标按**同一视口宽度**重新计算折叠结果，与渲染时的模型同源；
    /// 禁用项不产生消息（与 iced 的 `button` 无 `on_press` 等价）。
    pub(crate) fn overflow_action_message(
        &self,
        index: usize,
        viewport_width: f32,
    ) -> Option<Message> {
        let arrangement_mode = self.sidebar.is_arrangement_route();
        let overflow = self
            .toolbar
            .compute_overflow(right_content_width(self, viewport_width), arrangement_mode);
        if !overflow.has_overflow() {
            return None;
        }
        let has_selection = self.editor.selected_notes_count() > 0;
        self.toolbar
            .overflow_menu_items(
                &overflow,
                has_selection,
                self.settings.display.language,
                arrangement_mode,
            )
            .get(index)
            .filter(|item| item.enabled)
            .map(|item| item.on_press.clone())
    }
}

/// 右侧内容区宽度（窗口逻辑宽 − 左侧栏），与生产 `responsive` 的 `size.width` 同源。
pub(super) fn right_content_width(root: &Root, viewport_width: f32) -> f32 {
    let sidebar = 48.0
        + if root.sidebar.panel_visible {
            root.sidebar.panel_width
        } else {
            0.0
        };
    (viewport_width - sidebar).max(0.0)
}

/// 折叠结果 → 分组可见性。
pub(super) fn groups(overflow: &ToolbarOverflow) -> ToolbarGroupsModel {
    let visible = |group: ToolbarGroup| overflow.visible_groups.contains(&group);
    ToolbarGroupsModel {
        record: visible(ToolbarGroup::Record),
        playback: visible(ToolbarGroup::Playback),
        looping: visible(ToolbarGroup::Loop),
        undo_redo: visible(ToolbarGroup::UndoRedo),
        dashboard: visible(ToolbarGroup::Dashboard),
        tools: visible(ToolbarGroup::Tools),
        onion: visible(ToolbarGroup::OnionSkin),
        auto_scroll: visible(ToolbarGroup::AutoScroll),
    }
}

/// 工具区可见条目数（`ALL` 前缀）；走带模式固定四枚、不参与逐项折叠。
pub(super) fn visible_tools(overflow: &ToolbarOverflow, arrangement_mode: bool) -> usize {
    if arrangement_mode {
        usize::MAX
    } else {
        overflow.visible_tools.len()
    }
}

/// 溢出菜单是否打开 → egui 模型。
pub(super) fn overflow_menu(
    toolbar: &toolbar::Toolbar,
    overflow: &ToolbarOverflow,
    has_selection: bool,
    language: lumino_extras::i18n::Language,
    arrangement_mode: bool,
) -> Option<OverflowMenuModel> {
    if !toolbar.overflow_menu_open || !overflow.has_overflow() {
        return None;
    }
    Some(OverflowMenuModel {
        items: toolbar
            .overflow_menu_items(overflow, has_selection, language, arrangement_mode)
            .into_iter()
            .map(|item| OverflowItemModel {
                icon: overflow_icon(item.icon),
                tooltip: item.tooltip.to_owned(),
                enabled: item.enabled,
            })
            .collect(),
    })
}

/// 生产图标 → egui 图标。
///
/// 溢出菜单当前只用到下列图标；新增条目若引入新图标会落到兜底并触发 debug
/// 断言（发布版仍画一个占位图标，不至于让菜单缺项）。
fn overflow_icon(icon: crate::resources::icon::Icon) -> Icon {
    use crate::resources::icon::Icon as I;
    match icon {
        I::PlayCircle => Icon::PlayCircle,
        I::SkipBackward => Icon::SkipBackward,
        I::SkipForward => Icon::SkipForward,
        I::Play => Icon::Play,
        I::Pause => Icon::Pause,
        I::ArrowsLeftRight => Icon::ArrowsLeftRight,
        I::Ban => Icon::Ban,
        I::Undo => Icon::Undo,
        I::Redo => Icon::Redo,
        I::Clock => Icon::Clock,
        I::OnionSkin => Icon::OnionSkin,
        I::Scroll => Icon::Scroll,
        I::Users => Icon::Users,
        I::MousePointer => Icon::MousePointer,
        I::MousePointerYSelect => Icon::MousePointerYSelect,
        I::Pencil => Icon::Pencil,
        I::Eraser => Icon::Eraser,
        I::DrawToolbox => Icon::DrawToolbox,
        I::Quantize => Icon::Quantize,
        I::Speed => Icon::Speed,
        I::FlipVertical => Icon::FlipVertical,
        I::FlipHorizontal => Icon::FlipHorizontal,
        I::Split => Icon::Split,
        I::Glue => Icon::Glue,
        I::TransposeDown => Icon::TransposeDown,
        I::TransposeUp => Icon::TransposeUp,
        I::Tie => Icon::Tie,
        I::Curve => Icon::Curve,
        other => {
            debug_assert!(false, "溢出菜单图标未映射：{other:?}");
            Icon::EllipsisVertical
        }
    }
}
