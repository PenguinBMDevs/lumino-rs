//! 自定义标题栏：菜单、拖动区、窗口控制按钮。
//!
//! 几何取自生产 旧版 标题栏实测（`tests/ui-baseline/legacy/main_chrome-dpi100.png`）：
//! logo 区 48 宽、菜单项 = 文本宽 + 17（中文 14px 下即 45，与基线 45px 节距一致）、
//! 右侧三个窗口按钮各 45×29。

use egui::{Align, FontId, Layout};

use egui_extras::{Size, StripBuilder};

use super::{ChromeState, TitlebarModel, menu as menu_panel};
use crate::icons::{Icon, IconCache};
use crate::layout;
use crate::theme;
use crate::widgets;

/// 标题栏动作（主窗口与对话框 popup 共用）。
///
/// `lumino-ui-egui` 不认识生产 `window::Event`，只回传语义动作；生产侧再翻译。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitlebarAction {
    /// 按下拖动区：开始拖动窗口。
    Drag,
    /// 双击拖动区：切换最大化。
    ToggleMaximize,
    /// 最小化。
    Minimize,
    /// 关闭（走生产关闭流程，含未保存确认）。
    Close,
}

const ACTION_ID: &str = "lumino.titlebar.action";

/// 记录本帧标题栏动作（一帧一条）。
pub fn push_titlebar_action(ctx: &egui::Context, action: TitlebarAction) {
    if ctx
        .data(|d| d.get_temp::<Option<TitlebarAction>>(egui::Id::unique(ACTION_ID)))
        .flatten()
        .is_some()
    {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(ACTION_ID), Some(action)));
}

/// 取走本帧标题栏动作。
pub fn take_titlebar_action(ctx: &egui::Context) -> Option<TitlebarAction> {
    ctx.data_mut(|d| d.remove_temp::<Option<TitlebarAction>>(egui::Id::unique(ACTION_ID)))
        .flatten()
}

/// logo 区宽度（生产 `logo::view` 固定 48）。
const LOGO_W: f32 = 48.0;
/// 菜单项左右内边距合计（45 − 28 = 17，实测）。
const MENU_ITEM_PAD_X: f32 = 17.0;
/// 菜单文本字号（生产 `Text::new` 默认 14）。
const MENU_FONT: f32 = 14.0;
/// 窗口控制按钮区宽度。
const TRAFFIC_TOTAL_W: f32 = layout::TRAFFIC_BTN_W * 3.0;

/// 草稿场景入口：固定中文菜单、聚焦态。
pub fn show(ui: &mut egui::Ui, state: &mut ChromeState) {
    let model = TitlebarModel {
        focused: true,
        menus: vec![
            "文件".to_owned(),
            "编辑".to_owned(),
            "视图".to_owned(),
            "帮助".to_owned(),
        ],
        menu: None,
        menu_only: false,
    };
    show_model(ui, &model, &mut state.icons);
}

/// 按真实状态渲染标题栏。
pub fn show_model(ui: &mut egui::Ui, model: &TitlebarModel, icons: &mut IconCache) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(
        rect,
        0,
        if model.focused {
            theme::titlebar_bg()
        } else {
            theme::route_bar_bg()
        },
    );

    // 菜单项宽度随文本宽度变化（英文菜单比中文宽），逐项测量后布局。
    let labels = menu_panel::labels(model);
    let widths: Vec<f32> = labels
        .iter()
        .map(|label| text_width(ui, label) + MENU_ITEM_PAD_X)
        .collect();

    if model.menu_only {
        // 系统原生标题栏：客户端顶部只有 30px 菜单栏（旧版 `view_native_titlebar`
        // 的占位行），菜单从 x=0 开始，没有 logo / 窗口控制 / 拖动区。
        ui.horizontal_centered(|ui| {
            let _ = menu_panel::show_labels(ui, model, &widths);
        });
        return;
    }

    let menu_total: f32 = LOGO_W + widths.iter().sum::<f32>();

    StripBuilder::new(ui)
        .size(Size::exact(menu_total))
        .size(Size::remainder())
        .size(Size::exact(TRAFFIC_TOTAL_W))
        .horizontal(|mut strip| {
            strip.cell(|ui| menu(ui, model, &widths, icons));
            strip.cell(|ui| {
                // 拖动区：按下拖动 → Drag；双击 → 切换最大化。
                let rect = ui.max_rect();
                let response = ui.interact(
                    rect,
                    egui::Id::unique("lumino.titlebar.drag"),
                    egui::Sense::click_and_drag(),
                );
                if response.drag_started() {
                    push_titlebar_action(ui.ctx(), TitlebarAction::Drag);
                }
                if response.double_clicked() {
                    push_titlebar_action(ui.ctx(), TitlebarAction::ToggleMaximize);
                }
                ui.allocate_space(ui.available_size());
            });
            strip.cell(|ui| traffic(ui, icons));
        });
}

fn text_width(ui: &egui::Ui, label: &str) -> f32 {
    ui.painter()
        .layout_no_wrap(
            label.to_owned(),
            FontId::proportional(MENU_FONT),
            theme::text(),
        )
        .size()
        .x
}

fn menu(ui: &mut egui::Ui, model: &TitlebarModel, widths: &[f32], icons: &mut IconCache) {
    ui.horizontal_centered(|ui| {
        ui.add_space(15.0);
        ui.add(icons.default_image(ui.ctx(), Icon::LogoInApp, 18.0));
        ui.add_space(15.0);
        // 标签支持点击/悬停；矩形写回 egui 内存，供下拉面板定位。
        let _ = menu_panel::show_labels(ui, model, widths);
    });
}

fn traffic(ui: &mut egui::Ui, icons: &mut IconCache) {
    ui.spacing_mut().item_spacing.x = 0.0;
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let close = widgets::traffic_button(ui, icons, Icon::WindowClose, true);
        let max = widgets::traffic_button(ui, icons, Icon::WindowMax, false);
        let min = widgets::traffic_button(ui, icons, Icon::WindowMin, false);
        if close.clicked() {
            push_titlebar_action(ui.ctx(), TitlebarAction::Close);
        }
        if max.clicked() {
            push_titlebar_action(ui.ctx(), TitlebarAction::ToggleMaximize);
        }
        if min.clicked() {
            push_titlebar_action(ui.ctx(), TitlebarAction::Minimize);
        }
    });
}
