//! 主窗口浮层：通知、悬浮面板、菜单与画布 / UI 覆盖层。
//!
//! 分两层且顺序不可交换（DRAW-047）：画布覆盖层先画，UI 层后画。
//! 新增覆盖层先判定它属于哪一层，再插进对应的 `show_*` 函数，不要追加到末尾。

use crate::icons::IconCache;

use super::channels::{
    area_palette, bend_buttons_model, curve_preview_model, editor_buttons, editor_overlay_model,
    egg_model, help_tip_model, i2m_region_model, marquee_model, mask_panel, piano_roll_menu,
    push_help_tip_action, push_mask_panel_action, remote_cursors_model, shape_edit, shape_list,
    shape_preview_model, take_text_tool_model, toast_model, video_preview, window_rect,
};
use super::model::ChromeModel;
use super::{egg, menu, overflow, toast, tool_panel, toolbar_popup};

/// 绘制全部覆盖层，调用顺序即层级。
pub(super) fn show(ui: &egui::Ui, root: &mut egui::Ui, model: &ChromeModel, icons: &mut IconCache) {
    // ── 覆盖层判定（DRAW-045）──────────────────────────────────
    // 需要用户**独立决策**的确认 / 输入 / 设置类一律走独立 OS 窗口
    // （`DialogType` + `DialogManager`，见 `dialog_layer`）。
    // 以下覆盖层按规则保留：锚定参数面板（遮罩）、悬浮工具条、上下文/下拉
    // 菜单、非模态提示（Toast / 帮助提示 / 彩蛋）与画布预览。
    // 混音台已迁为 `DialogType::Mixer` 独立窗口，不在主窗口内绘制。
    // 新增覆盖层时必须先对照这条规则，而不是默认「先画在主窗口里」。
    show_notifications(root, icons);
    show_floating_panels(root);
    show_menus(root, model, icons);
    show_canvas_overlays(ui, root);
    show_ui_overlays(root, model, icons);
    show_context_menu(root, icons);
}

/// 右下角浮层：Toast 通知 + 首次使用帮助提示 + 「关于」页彩蛋。
///
/// 上一代视图（`toast::view` / `view/help_tip.rs`）已删除，这里独占绘制。
fn show_notifications(root: &mut egui::Ui, icons: &mut IconCache) {
    let full = window_rect(root);
    if let Some(toast) = toast_model(root.ctx()) {
        toast::show_toasts(root, full, &toast);
    }
    if let Some(tip) = help_tip_model(root.ctx())
        && let Some(action) = toast::show_help_tip(root, full, &tip)
    {
        push_help_tip_action(root.ctx(), action);
    }
    show_egg_layer(root, icons);
}

/// 锚定浮层：遮罩参数面板与纹理预览。
fn show_floating_panels(root: &mut egui::Ui) {
    // 遮罩参数面板：锚在鼠标右下，点面板外关闭；打开期间整窗占用。
    if let Some(mask) = mask_panel(root.ctx()) {
        let full = window_rect(root);
        if let Some(action) = crate::mask_panel::show(root, full, &mask) {
            push_mask_panel_action(root.ctx(), action);
        }
    }

    // 纹理预览（渲染器 16:9 卡片 / 全屏瀑布流）：画在所有 band 之后、
    // 浮层之前——内容区在 band 中央，但必须盖住其下的 上一代 占位。
    if let Some(preview) = video_preview(root.ctx()) {
        crate::video_preview::show(root, &preview);
    }
}

/// 菜单与弹层：最后画，不受标题栏 30px 裁剪，且盖在所有 band 之上。
fn show_menus(root: &mut egui::Ui, model: &ChromeModel, icons: &mut IconCache) {
    if let Some(titlebar) = &model.titlebar {
        menu::show_overlay(root, titlebar);
    }
    // 工具栏弹层同样画在 band 之外（向下展开会盖住卷帘区）。
    if let Some(toolbar) = &model.toolbar {
        toolbar_popup::show_overlay(root, toolbar);
        overflow::show_overlay(root, toolbar, icons);
    }
}

/// 画布层：全部画布覆盖层先于 UI 层绘制（DRAW-047）。
///
/// 形状 / 曲线 / 刀框 / 框选等预览只能用 `content_bounds` 裁剪，不得压住
/// 悬浮工具条、工具栏弹层与标题栏菜单；UI 层统一后画。
fn show_canvas_overlays(ui: &egui::Ui, root: &mut egui::Ui) {
    // 图片转 MIDI 区域框（Placing）：在预览之下。
    if let Some(region) = i2m_region_model(root.ctx()) {
        crate::image_to_midi::show(root, &region);
    }

    // 选中框 / 编辑框 / 8+1 锚点（020 · 041C）：画在预览之下。
    if let Some(edit) = shape_edit(root.ctx()) {
        crate::shape_edit::show(root, &edit);
    }

    // 曲线工具预览（路径 / 锚点 / 控制柄 / 产物块）。
    if let Some(preview) = curve_preview_model(root.ctx()) {
        crate::curve_preview::show(root, &preview);
    }

    // 形状工具预览（待确认图形 + 拖拽）：在框选框之上、按钮之下。
    if let Some(preview) = shape_preview_model(root.ctx()) {
        crate::shape_preview::show(root, &preview);
    }

    // 编辑器叠加层预览（切割线 / 刀框 / 角度柔化）。
    // 沿用外层 `ui` 的裁剪矩形：模型未带裁剪时以调用方 Ui 为准。
    if let Some(preview) = editor_overlay_model(root.ctx()) {
        crate::editor_overlay::show(ui, &preview);
    }

    // 协作远程光标：最上层，避免被预览/框选盖住。裁剪口径同上。
    if let Some(cursors) = remote_cursors_model(root.ctx()) {
        crate::remote_cursors::show(ui, &cursors);
    }

    // 框选框（拖拽中 / 选中音符包围盒）：由 egui 绘制，
    // 画布的绘制层被底层纹理覆盖后不可见。
    if let Some(marquee) = marquee_model(root.ctx()) {
        crate::marquee::show(root, &marquee);
    }
}

/// UI 层：永远画在全部画布覆盖层之上（DRAW-047）。
fn show_ui_overlays(root: &mut egui::Ui, model: &ChromeModel, icons: &mut IconCache) {
    // 悬浮绘制工具条：锚定面板要盖在卷帘与工具栏弹层之上；它自身用不透明底
    // 接管该区域，不依赖画布预览的裁剪。
    if let Some(tool_panel) = &model.tool_panel {
        tool_panel::show_overlay(root, tool_panel, icons);
    } else {
        tool_panel::clear_triggers(root.ctx());
    }

    // 文字工具覆盖层（文本框 / 字形预览 / 输入框 / √× 模式按钮）。
    // 取走模型（输入框需要 `&mut` 文字），下一帧生产侧会重新 set。
    if let Some(mut model) = take_text_tool_model(root.ctx()) {
        if let Some(action) = crate::text_tool::show(root, &mut model, icons) {
            crate::text_tool::push_action(root.ctx(), action);
        }
        let rects = model.blocking_rects();
        if !rects.is_empty() {
            menu::extend_blocked(root.ctx(), rects);
        }
    }

    // 曲线锚点弯折按钮（曲线 / 折线）：在曲线预览之上、√× 之下。
    if let Some(buttons) = bend_buttons_model(root.ctx()) {
        if let Some(bend) = crate::bend_buttons::show(root, &buttons) {
            crate::bend_buttons::push_action(root.ctx(), bend);
        }
        let rects: Vec<egui::Rect> = buttons.buttons.iter().map(|button| button.rect).collect();
        if !rects.is_empty() {
            menu::extend_blocked(root.ctx(), rects);
        }
    }

    // 编辑器悬浮 √× 按钮（形状 / I2M / 曲线 / 画刷）：盖在卷帘之上，
    // 命中区加入占用，避免按下按钮的同一击又在画布上画音符。
    if let Some(buttons) = editor_buttons(root.ctx()) {
        if let Some(action) = crate::editor_action_buttons::show(root, &buttons, icons) {
            crate::editor_action_buttons::push_action(root.ctx(), action);
        }
        let rects: Vec<egui::Rect> = buttons.buttons.iter().map(|button| button.rect).collect();
        if !rects.is_empty() {
            menu::extend_blocked(root.ctx(), rects);
        }
    }

    // 形状列表（只读）与区域着色调色板：只绘制，点击仍由画布命中分派。
    if let Some(list) = shape_list(root.ctx()) {
        crate::shape_list::show(root, &list);
    }
    if let Some(palette) = area_palette(root.ctx()) {
        crate::area_palette::show(root, &palette);
    }
}

/// 钢琴卷帘右键菜单：最顶层（盖住卷帘与工具弹层）。
///
/// 打开期间整块画布加入占用矩形：与 上一代 旧 `background_close_overlay` 同语义，
/// 关闭用的那一次点击不会同时落到画布上。
fn show_context_menu(root: &mut egui::Ui, icons: &mut IconCache) {
    let Some(menu) = piano_roll_menu(root.ctx()) else {
        return;
    };
    if let Some(action) = crate::piano_roll_context_menu::show(root, &menu, icons) {
        crate::piano_roll_context_menu::push_action(root.ctx(), action);
    }
    menu::extend_blocked(root.ctx(), [menu.available]);
}

/// 绘制「关于」页 logo 彩蛋飞行层（主窗口与独立对话框窗口共用）。
///
/// 对话框接管层会在绘制完对话框内容后提前 `return`，因此设置窗口的「关于」页
/// 必须在 `return` 之前调用本函数；主窗口则在通用覆盖层区调用。
pub(super) fn show_egg_layer(ui: &egui::Ui, icons: &mut IconCache) {
    if let Some(egg) = egg_model(ui.ctx()) {
        egg::show(ui, window_rect(ui), &egg, icons);
    }
}
