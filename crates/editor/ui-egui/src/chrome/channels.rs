//! 主窗口的 `ctx.data` 通道与窗口几何。
//!
//! 生产侧每帧把模型写进 `ctx.data_mut`（`set_*`），绘制阶段再读回
//! （`*_model` / `take_*`）。写读分离是为了让「数据搬运」与「绘制编排」解耦：
//! 通道集中在本文件，`bands` / `overlays` 只关心绘制顺序与几何。

use super::egg::EggModel;
use super::first_screen::FirstScreenModel;
use super::model::{ArrangementModel, HelpTipModel, TexturePreviewModel, ToastModel};

/// 记录本帧真实窗口逻辑尺寸（由宿主提供）。
///
/// 路由栏底部按钮必须按**窗口高度**锚定：`ui.max_rect()` / `viewport_rect()`
/// 在某些 frame 会给出无界值（实测有一趟为 7980），按它算会把底部按钮推出屏幕。
pub fn set_window_size(ctx: &egui::Context, size: egui::Vec2) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("lumino.window-size"), size));
}

/// 取本帧真实窗口逻辑尺寸。
pub(crate) fn window_size(ctx: &egui::Context) -> Option<egui::Vec2> {
    ctx.data(|d| d.get_temp::<egui::Vec2>(egui::Id::unique("lumino.window-size")))
}

/// 本帧的窗口逻辑矩形（原点 0,0；优先用宿主注入的真实尺寸）。
///
/// 三条 band 都贴窗口边（工具栏铺满、左/右栏贴两侧），直接依赖 `root.max_rect()`
/// 会隐含「Ui 与窗口等大」这一前提；宿主已注入真实尺寸时以它为准更稳。
/// 未注入时（如 `capture_egui` 的离屏 harness）回退到 `max_rect`，行为不变。
pub(super) fn window_rect(ui: &egui::Ui) -> egui::Rect {
    window_size(ui.ctx())
        .map(|size| egui::Rect::from_min_size(egui::Pos2::ZERO, size))
        .unwrap_or_else(|| ui.max_rect())
}

/// 记录本帧「无面板」默认第一屏模型（`None` = 不显示）。
pub fn set_first_screen(ctx: &egui::Context, model: Option<FirstScreenModel>) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("lumino.first-screen"), model));
}

/// 本帧「无面板」默认第一屏模型（`None` = 不显示）。
pub(super) fn first_screen_model(ctx: &egui::Context) -> Option<FirstScreenModel> {
    ctx.data(|d| d.get_temp::<Option<FirstScreenModel>>(egui::Id::unique("lumino.first-screen")))
        .flatten()
}

/// 记录本帧的遮罩参数面板模型（`None` = 面板未打开）。
pub fn set_mask_panel(ctx: &egui::Context, model: Option<crate::mask_panel::MaskPanelModel>) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("lumino.mask-panel"), model));
}

/// 取本帧的遮罩参数面板模型。
pub(super) fn mask_panel(ctx: &egui::Context) -> Option<crate::mask_panel::MaskPanelModel> {
    ctx.data(|d| {
        d.get_temp::<Option<crate::mask_panel::MaskPanelModel>>(egui::Id::unique(
            "lumino.mask-panel",
        ))
    })
    .flatten()
}

/// 记录本帧的纹理预览模型（渲染器面板 16:9 卡片 / 全屏瀑布流）。
pub fn set_video_preview(ctx: &egui::Context, model: Option<TexturePreviewModel>) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("lumino.video-preview"), model));
}

/// 取本帧的纹理预览模型。
pub(super) fn video_preview(ctx: &egui::Context) -> Option<TexturePreviewModel> {
    ctx.data(|d| {
        d.get_temp::<Option<TexturePreviewModel>>(egui::Id::unique("lumino.video-preview"))
    })
    .flatten()
}

/// 记录本帧的走带标尺模型（`None` = 不接管或不在走带路由）。
pub fn set_arrangement(ctx: &egui::Context, model: Option<ArrangementModel>) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("lumino.arrangement"), model));
}

/// 取走本帧的走带标尺模型。
pub(super) fn arrangement_model(ctx: &egui::Context) -> Option<ArrangementModel> {
    ctx.data(|d| d.get_temp::<Option<ArrangementModel>>(egui::Id::unique("lumino.arrangement")))
        .flatten()
}

/// 记录本帧的视频剪辑时间轴模型（`None` = 不接管或不在渲染器入口）。
pub fn set_video_timeline(
    ctx: &egui::Context,
    model: Option<(egui::Rect, crate::video_clip_timeline::VideoTimelineModel)>,
) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("lumino.video-timeline"), model));
}

/// 取走本帧的视频剪辑时间轴模型。
pub(super) fn video_timeline(
    ctx: &egui::Context,
) -> Option<(egui::Rect, crate::video_clip_timeline::VideoTimelineModel)> {
    ctx.data(|d| {
        d.get_temp::<Option<(egui::Rect, crate::video_clip_timeline::VideoTimelineModel)>>(
            egui::Id::unique("lumino.video-timeline"),
        )
    })
    .flatten()
}

/// 记录本帧的底层画布纹理（`None` = 不画底层，走原通道）。
pub fn set_base_texture(ctx: &egui::Context, id: Option<egui::TextureId>) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("lumino.base-texture"), id));
}

/// 本帧是否由 egui 采样底层画布纹理。
pub fn base_texture_active(ctx: &egui::Context) -> bool {
    base_texture(ctx).is_some()
}

/// 取走本帧的底层画布纹理。
pub(super) fn base_texture(ctx: &egui::Context) -> Option<egui::TextureId> {
    ctx.data(|d| d.get_temp::<Option<egui::TextureId>>(egui::Id::unique("lumino.base-texture")))
        .flatten()
}

/// 时间轴动作通道（一帧可多条）。
const VIDEO_TIMELINE_ACTION_ID: &str = "lumino.video-timeline.action";

/// 追加本帧时间轴动作。
pub(crate) fn push_video_timeline_action(
    ctx: &egui::Context,
    action: crate::video_clip_timeline::VideoTimelineAction,
) {
    let mut actions = ctx
        .data(|d| {
            d.get_temp::<Vec<crate::video_clip_timeline::VideoTimelineAction>>(egui::Id::unique(
                VIDEO_TIMELINE_ACTION_ID,
            ))
        })
        .unwrap_or_default();
    actions.push(action);
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(VIDEO_TIMELINE_ACTION_ID), actions));
}

/// 取走本帧时间轴动作。
pub fn take_video_timeline_actions(
    ctx: &egui::Context,
) -> Vec<crate::video_clip_timeline::VideoTimelineAction> {
    ctx.data_mut(|d| {
        d.remove_temp::<Vec<crate::video_clip_timeline::VideoTimelineAction>>(egui::Id::unique(
            VIDEO_TIMELINE_ACTION_ID,
        ))
    })
    .unwrap_or_default()
}

/// 遮罩面板动作通道。
const MASK_PANEL_ACTION_ID: &str = "lumino.mask-panel.action";

pub(super) fn push_mask_panel_action(
    ctx: &egui::Context,
    action: crate::mask_panel::MaskPanelAction,
) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(MASK_PANEL_ACTION_ID), action));
}

/// 取走本帧遮罩面板动作。
pub fn take_mask_panel_action(ctx: &egui::Context) -> Option<crate::mask_panel::MaskPanelAction> {
    let action = ctx.data(|d| {
        d.get_temp::<crate::mask_panel::MaskPanelAction>(egui::Id::unique(MASK_PANEL_ACTION_ID))
    });
    if action.is_some() {
        ctx.data_mut(|d| {
            d.remove::<crate::mask_panel::MaskPanelAction>(egui::Id::unique(MASK_PANEL_ACTION_ID))
        });
    }
    action
}

/// 帮助提示动作通道。
const HELP_TIP_ACTION_ID: &str = "lumino.help-tip.action";

pub(super) fn push_help_tip_action(
    ctx: &egui::Context,
    action: crate::chrome::toast::HelpTipAction,
) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(HELP_TIP_ACTION_ID), action));
}

/// 取走本帧帮助提示动作。
pub fn take_help_tip_action(ctx: &egui::Context) -> Option<crate::chrome::toast::HelpTipAction> {
    let action = ctx.data(|d| {
        d.get_temp::<crate::chrome::toast::HelpTipAction>(egui::Id::unique(HELP_TIP_ACTION_ID))
    });
    if action.is_some() {
        ctx.data_mut(|d| {
            d.remove::<crate::chrome::toast::HelpTipAction>(egui::Id::unique(HELP_TIP_ACTION_ID))
        });
    }
    action
}

/// 记录本帧 Toast 卡片（None = 不显示）。
pub fn set_toast(ctx: &egui::Context, model: Option<ToastModel>) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("lumino.toast"), model));
}

pub(super) fn toast_model(ctx: &egui::Context) -> Option<ToastModel> {
    ctx.data(|d| d.get_temp::<Option<ToastModel>>(egui::Id::unique("lumino.toast")))
        .flatten()
}

/// 记录本帧首次使用帮助提示（None = 不显示）。
pub fn set_help_tip(ctx: &egui::Context, model: Option<HelpTipModel>) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("lumino.help-tip"), model));
}

pub(super) fn help_tip_model(ctx: &egui::Context) -> Option<HelpTipModel> {
    ctx.data(|d| d.get_temp::<Option<HelpTipModel>>(egui::Id::unique("lumino.help-tip")))
        .flatten()
}

/// 记录本帧「关于」页 logo 彩蛋飞行层（None = 不在飞行中）。
pub fn set_egg(ctx: &egui::Context, model: Option<EggModel>) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("lumino.about-egg"), model));
}

pub(super) fn egg_model(ctx: &egui::Context) -> Option<EggModel> {
    ctx.data(|d| d.get_temp::<Option<EggModel>>(egui::Id::unique("lumino.about-egg")))
        .flatten()
}

/// 记录本帧钢琴卷帘右键菜单（`None` = 关闭）。
pub fn set_piano_roll_menu(
    ctx: &egui::Context,
    model: Option<crate::piano_roll_context_menu::PianoRollMenuModel>,
) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("lumino.piano-roll-menu"), model));
}

pub(super) fn piano_roll_menu(
    ctx: &egui::Context,
) -> Option<crate::piano_roll_context_menu::PianoRollMenuModel> {
    ctx.data(|d| {
        d.get_temp::<Option<crate::piano_roll_context_menu::PianoRollMenuModel>>(egui::Id::unique(
            "lumino.piano-roll-menu",
        ))
    })
    .flatten()
}

/// 记录本帧编辑器悬浮 √× 按钮（空模型 = 不显示）。
pub fn set_editor_buttons(
    ctx: &egui::Context,
    model: crate::editor_action_buttons::EditorButtonsModel,
) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("lumino.editor-buttons"), model));
}

pub(super) fn editor_buttons(
    ctx: &egui::Context,
) -> Option<crate::editor_action_buttons::EditorButtonsModel> {
    ctx.data(|d| {
        d.get_temp::<crate::editor_action_buttons::EditorButtonsModel>(egui::Id::unique(
            "lumino.editor-buttons",
        ))
    })
    .filter(|model| !model.is_empty())
}

/// 记录本帧框选框（空模型 = 不显示）。
pub fn set_marquee(ctx: &egui::Context, model: crate::marquee::MarqueeModel) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("lumino.marquee"), model));
}

pub(super) fn marquee_model(ctx: &egui::Context) -> Option<crate::marquee::MarqueeModel> {
    ctx.data(|d| d.get_temp::<crate::marquee::MarqueeModel>(egui::Id::unique("lumino.marquee")))
        .filter(|model| !model.is_empty())
}

/// 记录本帧形状工具预览（空模型 = 不显示）。
pub fn set_shape_preview(ctx: &egui::Context, model: crate::shape_preview::ShapePreviewModel) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("lumino.shape-preview"), model));
}

pub(super) fn shape_preview_model(
    ctx: &egui::Context,
) -> Option<crate::shape_preview::ShapePreviewModel> {
    ctx.data(|d| {
        d.get_temp::<crate::shape_preview::ShapePreviewModel>(egui::Id::unique(
            "lumino.shape-preview",
        ))
    })
    .filter(|model| !model.is_empty())
}

/// 记录本帧选中框 / 编辑框 / 锚点（空模型 = 不显示）。
pub fn set_shape_edit(ctx: &egui::Context, model: crate::shape_edit::ShapeEditModel) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("lumino.shape-edit"), model));
}

pub(super) fn shape_edit(ctx: &egui::Context) -> Option<crate::shape_edit::ShapeEditModel> {
    ctx.data(|d| {
        d.get_temp::<crate::shape_edit::ShapeEditModel>(egui::Id::unique("lumino.shape-edit"))
    })
    .filter(|model| !model.is_empty())
}

/// 记录本帧形状列表（空模型 = 不显示）。
pub fn set_shape_list(ctx: &egui::Context, model: crate::shape_list::ShapeListModel) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("lumino.shape-list"), model));
}

pub(super) fn shape_list(ctx: &egui::Context) -> Option<crate::shape_list::ShapeListModel> {
    ctx.data(|d| {
        d.get_temp::<crate::shape_list::ShapeListModel>(egui::Id::unique("lumino.shape-list"))
    })
    .filter(|model| !model.is_empty())
}

/// 记录本帧区域着色调色板（空模型 = 不显示）。
pub fn set_area_palette(ctx: &egui::Context, model: crate::area_palette::AreaPaletteModel) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("lumino.area-palette"), model));
}

pub(super) fn area_palette(ctx: &egui::Context) -> Option<crate::area_palette::AreaPaletteModel> {
    ctx.data(|d| {
        d.get_temp::<crate::area_palette::AreaPaletteModel>(egui::Id::unique("lumino.area-palette"))
    })
    .filter(|model| !model.is_empty())
}

/// 记录本帧曲线预览（空模型 = 不显示）。
pub fn set_curve_preview(ctx: &egui::Context, model: crate::curve_preview::CurvePreviewModel) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("lumino.curve-preview"), model));
}

pub(super) fn curve_preview_model(
    ctx: &egui::Context,
) -> Option<crate::curve_preview::CurvePreviewModel> {
    ctx.data(|d| {
        d.get_temp::<crate::curve_preview::CurvePreviewModel>(egui::Id::unique(
            "lumino.curve-preview",
        ))
    })
    .filter(|model| !model.is_empty())
}

/// 记录本帧远程光标（空模型 = 不显示）。
pub fn set_remote_cursors(ctx: &egui::Context, model: crate::remote_cursors::RemoteCursorModel) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("lumino.remote-cursors"), model));
}

pub(super) fn remote_cursors_model(
    ctx: &egui::Context,
) -> Option<crate::remote_cursors::RemoteCursorModel> {
    ctx.data(|d| {
        d.get_temp::<crate::remote_cursors::RemoteCursorModel>(egui::Id::unique(
            "lumino.remote-cursors",
        ))
    })
    .filter(|model| !model.is_empty())
}

/// 记录本帧编辑器叠加层预览（空模型 = 不显示）。
pub fn set_editor_overlay(ctx: &egui::Context, model: crate::editor_overlay::EditorOverlayModel) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("lumino.editor-overlay"), model));
}

pub(super) fn editor_overlay_model(
    ctx: &egui::Context,
) -> Option<crate::editor_overlay::EditorOverlayModel> {
    ctx.data(|d| {
        d.get_temp::<crate::editor_overlay::EditorOverlayModel>(egui::Id::unique(
            "lumino.editor-overlay",
        ))
    })
    .filter(|model| !model.is_empty())
}

/// 记录本帧 I2M 区域框（`None` = 不显示）。
pub fn set_i2m_region(ctx: &egui::Context, model: crate::image_to_midi::I2mRegionModel) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("lumino.i2m-region"), model));
}

pub(super) fn i2m_region_model(
    ctx: &egui::Context,
) -> Option<crate::image_to_midi::I2mRegionModel> {
    ctx.data(|d| {
        d.get_temp::<crate::image_to_midi::I2mRegionModel>(egui::Id::unique("lumino.i2m-region"))
    })
    .filter(|model| model.rect.is_some())
}

/// 记录本帧曲线弯折按钮（空模型 = 不显示）。
pub fn set_bend_buttons(ctx: &egui::Context, model: crate::bend_buttons::BendButtonsModel) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("lumino.bend-buttons"), model));
}

pub(super) fn bend_buttons_model(
    ctx: &egui::Context,
) -> Option<crate::bend_buttons::BendButtonsModel> {
    ctx.data(|d| {
        d.get_temp::<crate::bend_buttons::BendButtonsModel>(egui::Id::unique("lumino.bend-buttons"))
    })
    .filter(|model| !model.is_empty())
}

/// 记录本帧力度面板骨架（`None` = 面板不可见）。
pub fn set_velocity_panel(
    ctx: &egui::Context,
    model: Option<crate::velocity_panel::VelocityPanelModel>,
) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("lumino.velocity-panel"), model));
}

pub(super) fn velocity_panel_model(
    ctx: &egui::Context,
) -> Option<crate::velocity_panel::VelocityPanelModel> {
    ctx.data(|d| {
        d.get_temp::<Option<crate::velocity_panel::VelocityPanelModel>>(egui::Id::unique(
            "lumino.velocity-panel",
        ))
    })
    .flatten()
}

/// 记录本帧文字工具覆盖层（`None` = 不显示）。
pub fn set_text_tool(ctx: &egui::Context, model: Option<crate::text_tool::TextToolModel>) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("lumino.text-tool"), model));
}

pub(super) fn take_text_tool_model(ctx: &egui::Context) -> Option<crate::text_tool::TextToolModel> {
    ctx.data_mut(|d| {
        d.remove_temp::<Option<crate::text_tool::TextToolModel>>(egui::Id::unique(
            "lumino.text-tool",
        ))
    })
    .flatten()
    .filter(|model| !model.is_empty())
}

/// 记录本帧渲染器入口面板（`None` = 当前不是渲染器入口）。
pub fn set_renderer_panel(
    ctx: &egui::Context,
    model: Option<crate::renderer_panel::RendererPanelModel>,
) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("lumino.renderer-panel"), model));
}

pub(super) fn renderer_panel_model(
    ctx: &egui::Context,
) -> Option<crate::renderer_panel::RendererPanelModel> {
    ctx.data(|d| {
        d.get_temp::<Option<crate::renderer_panel::RendererPanelModel>>(egui::Id::unique(
            "lumino.renderer-panel",
        ))
    })
    .flatten()
}
