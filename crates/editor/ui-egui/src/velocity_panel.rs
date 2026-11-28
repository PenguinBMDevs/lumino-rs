//! 力度 / CC / Tempo 面板（egui 版骨架 + 工具栏）。
//!
//! 数据曲线（力度柱 / CC 折线 / Tempo 折线）由 `lumino-gfx` 直接画进离屏纹理，
//! 本模块只补 egui 侧被底层纹理覆盖的部分：
//! - 工具栏（模式按钮 / CC 下拉 / 信息文字）；
//! - 网格线（小节 / 拍 / 半拍）与刻度标签；
//! - 顶部 resize 手柄的视觉（拖拽命中仍在 上一代 Canvas，保持原行为）。
//!
//! 面板背景不铺色：gfx 的清屏色已是 `background.base`，铺色会盖掉 gfx 数据曲线。

use egui::{Align2, CornerRadius, FontId, Margin, Pos2, Rect, Stroke, StrokeKind, Vec2};
use lumino_message::CcOption;

use crate::theme;

mod action;
mod draw;
mod input;
mod types;

pub use action::{VelocityPanelAction, push_action, take_action};
pub use types::{
    AutomationAnchor, AutomationLaneModel, TempoPointModel, VelocityPointModel, VelocityView,
};

/// CC 下拉弹层展开状态（egui 临时数据键）。
const CC_OPEN_ID: &str = "lumino.velocity-panel.cc-open";

/// 面板工具栏高度（与 上一代 `TOOLBAR_HEIGHT` 一致）。
pub const TOOLBAR_HEIGHT: f32 = 28.0;
/// 顶部 resize 手柄高度。
const RESIZE_HANDLE: f32 = 5.0;

/// 垂直网格线强度。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStrength {
    /// 小节线。
    Measure,
    /// 拍线。
    Beat,
    /// 半拍线。
    HalfBeat,
}

/// 力度面板模型（窗口逻辑坐标）。
#[derive(Debug, Clone, PartialEq)]
pub struct VelocityPanelModel {
    /// 整块面板。
    pub panel: Rect,
    /// 工具栏。
    pub toolbar: Rect,
    /// 数据区（网格 / 曲线）。
    pub canvas: Rect,
    /// 模式按钮文案（力度 / 速度 / CC / Bend）。
    pub mode_label: String,
    /// 模式按钮是否用主色（力度 / 速度）。
    pub mode_primary: bool,
    /// 是否显示 CC 下拉。
    pub cc_mode: bool,
    /// 当前 CC 选中项文案。
    pub cc_selected: String,
    /// 右侧信息文字。
    pub info: String,
    /// 水平网格线（canvas 局部 y, 标签）。
    pub hlines: Vec<(f32, String)>,
    /// 垂直网格线（canvas 局部 x, 强度）。
    pub vlines: Vec<(f32, LineStrength)>,
    /// 力度模式 + 曲线工具：egui 接管力度编辑。
    pub velocity_editing: bool,
    /// 力度点快照。
    pub velocity_points: Vec<VelocityPointModel>,
    /// 是否存在选中集（曲线绘制只影响选中音符）。
    pub has_selection: bool,
    /// 选中的音符索引。
    pub selected_notes: Vec<usize>,
    /// 时间轴视图参数。
    pub view: VelocityView,
    /// Tempo 模式（决定是否绘制折线图，与工具无关）。
    pub tempo_mode: bool,
    /// Tempo 模式：egui 接管 Tempo 编辑。
    pub tempo_editing: bool,
    /// Tempo 控制点快照。
    pub tempo_points: Vec<TempoPointModel>,
    /// Tempo BPM 绘制上限。
    pub tempo_max_bpm: f64,
    /// Tempo 折线宽度。
    pub tempo_line_width: f32,
    /// 当前是否橡皮工具（点击删除 Tempo 点）。
    pub eraser_tool: bool,
    /// CC / Bend 模式：egui 接管锚点编辑。
    pub automation_editing: bool,
    /// 自动化 lane 快照（无 lane 时为 `None`：新锚点会创建 lane）。
    pub automation: Option<AutomationLaneModel>,
    /// 自动化垂直缩放（1.0 = 满量程）。
    pub value_zoom: f32,
    /// 自动化垂直滚动（值空间，面板顶部对应的值）。
    pub value_scroll: f32,
}

/// `show` 的跨区块可变状态：字段顺序与原函数的状态写入顺序一致。
struct ShowState {
    action: Option<VelocityPanelAction>,
    blocked: Vec<Rect>,
    cc_open: bool,
    cc_rect: Option<Rect>,
    popup_rect: Option<Rect>,
}

impl ShowState {
    fn new(model: &VelocityPanelModel) -> Self {
        Self {
            action: None,
            blocked: vec![model.toolbar, input::resize_zone(model.canvas)],
            cc_open: false,
            cc_rect: None,
            popup_rect: None,
        }
    }
}

/// 绘制面板骨架与工具栏。
///
/// 返回 `(动作, 需要计入占用列表的矩形)`：工具栏 / CC 弹层 / 各模式的编辑区
/// 与顶部 resize 手柄都必须占用；`Tempo` / `Velocity` / `CC` / `Bend` 的数据区
/// 编辑全部归 egui（resize 手柄由 `input::resize_zone` 独占）。
pub fn show(ui: &egui::Ui, model: &VelocityPanelModel) -> (Option<VelocityPanelAction>, Vec<Rect>) {
    let painter = ui.painter();
    let mut state = ShowState::new(model);
    let mode_rect = draw_toolbar(ui, painter, model, &mut state.action);
    draw_cc_selector(ui, painter, model, mode_rect, &mut state);
    draw_info_and_grab(painter, model);
    draw_grid(painter, model);

    // Tempo 折线图（gfx 只画力度/CC 曲线，Tempo 图由 egui 补）。
    if model.tempo_mode && !model.tempo_points.is_empty() {
        draw::tempo_graph(painter, model);
    }

    show_cc_popup(ui, model, &mut state);
    close_cc_popup(ui, model, &mut state);
    handle_data_area(ui, model, &mut state);

    (state.action, state.blocked)
}

/// 工具栏底色与模式按钮：返回模式按钮矩形，供 CC 选择器定位。
fn draw_toolbar(
    ui: &egui::Ui,
    painter: &egui::Painter,
    model: &VelocityPanelModel,
    action: &mut Option<VelocityPanelAction>,
) -> Rect {
    // 工具栏底色 + 底边线。
    painter.rect_filled(model.toolbar, CornerRadius::ZERO, theme::toolbar_bg());
    painter.line_segment(
        [
            Pos2::new(model.toolbar.left(), model.toolbar.bottom()),
            Pos2::new(model.toolbar.right(), model.toolbar.bottom()),
        ],
        Stroke::new(1.0, theme::border()),
    );

    // 模式按钮。
    let mode_rect = Rect::from_min_size(
        Pos2::new(model.toolbar.left() + 8.0, model.toolbar.top() + 4.0),
        Vec2::new(draw::text_width(ui, &model.mode_label, 12.0) + 16.0, 20.0),
    );
    let mode_fill = if model.mode_primary {
        theme::accent()
    } else {
        theme::base_bg()
    };
    let mode_text = if model.mode_primary {
        theme::on_accent()
    } else {
        theme::text()
    };
    painter.rect(
        mode_rect,
        CornerRadius::same(3),
        mode_fill,
        Stroke::NONE,
        StrokeKind::Inside,
    );
    painter.text(
        mode_rect.center(),
        Align2::CENTER_CENTER,
        &model.mode_label,
        FontId::proportional(12.0),
        mode_text,
    );
    let mode_resp = ui.interact(
        mode_rect,
        egui::Id::unique("lumino.velocity-panel.mode"),
        egui::Sense::click(),
    );
    if mode_resp
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        *action = Some(VelocityPanelAction::ToggleMode);
    }
    mode_rect
}

/// CC 选择器（自绘下拉：弹层矩形可精确占用，不会漏给 上一代）。
fn draw_cc_selector(
    ui: &egui::Ui,
    painter: &egui::Painter,
    model: &VelocityPanelModel,
    mode_rect: Rect,
    state: &mut ShowState,
) {
    state.cc_open = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(egui::Id::unique(CC_OPEN_ID)))
        .unwrap_or(false);
    if model.cc_mode {
        let rect = Rect::from_min_size(
            Pos2::new(mode_rect.right() + 8.0, model.toolbar.top() + 4.0),
            Vec2::new(170.0, 20.0),
        );
        state.cc_rect = Some(rect);
        painter.rect(
            rect,
            CornerRadius::same(3),
            theme::base_bg(),
            Stroke::new(1.0, theme::border()),
            StrokeKind::Inside,
        );
        painter.text(
            Pos2::new(rect.left() + 6.0, rect.center().y),
            Align2::LEFT_CENTER,
            &model.cc_selected,
            FontId::proportional(12.0),
            theme::text(),
        );
        painter.text(
            Pos2::new(rect.right() - 6.0, rect.center().y),
            Align2::RIGHT_CENTER,
            "▾",
            FontId::proportional(10.0),
            theme::text_muted(),
        );
        let cc_resp = ui.interact(
            rect,
            egui::Id::unique("lumino.velocity-panel.cc"),
            egui::Sense::click(),
        );
        if cc_resp
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            state.cc_open = !state.cc_open;
        }
        state.blocked.push(rect);
    }
}

/// 右侧信息文字与顶部 resize 手柄视觉。
fn draw_info_and_grab(painter: &egui::Painter, model: &VelocityPanelModel) {
    // 右侧信息。
    painter.text(
        Pos2::new(model.toolbar.right() - 8.0, model.toolbar.center().y),
        Align2::RIGHT_CENTER,
        &model.info,
        FontId::proportional(11.0),
        theme::text_muted().gamma_multiply(0.8),
    );

    // 顶部 resize 手柄视觉（命中仍在 上一代 Canvas）。
    let grab = Rect::from_center_size(
        Pos2::new(
            model.canvas.center().x,
            model.canvas.top() + RESIZE_HANDLE * 0.5,
        ),
        Vec2::new(40.0, 2.0),
    );
    painter.rect_filled(
        grab,
        CornerRadius::same(1),
        theme::text().gamma_multiply(0.35),
    );
}

/// 网格绘制。
fn draw_grid(painter: &egui::Painter, model: &VelocityPanelModel) {
    // 网格：垂直线（小节 / 拍 / 半拍）。
    for &(x, strength) in &model.vlines {
        let color = match strength {
            LineStrength::Measure => theme::text().gamma_multiply(0.45),
            LineStrength::Beat => theme::text().gamma_multiply(0.25),
            LineStrength::HalfBeat => theme::text().gamma_multiply(0.12),
        };
        let x = model.canvas.left() + x;
        painter.line_segment(
            [
                Pos2::new(x, model.canvas.top() + RESIZE_HANDLE),
                Pos2::new(x, model.canvas.bottom() - RESIZE_HANDLE),
            ],
            Stroke::new(1.0, color),
        );
    }
    // 水平网格 + 标签。
    for (y, label) in &model.hlines {
        let y = model.canvas.top() + *y;
        painter.line_segment(
            [
                Pos2::new(model.canvas.left() + 8.0, y),
                Pos2::new(model.canvas.right() - 8.0, y),
            ],
            Stroke::new(1.0, theme::text().gamma_multiply(0.18)),
        );
        painter.text(
            Pos2::new(model.canvas.left() + 4.0, y - 6.0),
            Align2::LEFT_TOP,
            label,
            FontId::proportional(9.0),
            theme::text().gamma_multiply(0.55),
        );
    }
}

/// CC 弹层（滚动列表，129 项）。
fn show_cc_popup(ui: &egui::Ui, model: &VelocityPanelModel, state: &mut ShowState) {
    if state.cc_open
        && let Some(cc_rect) = state.cc_rect
    {
        let popup_pos = Pos2::new(cc_rect.left(), cc_rect.bottom() + 2.0);
        let popup = egui::Area::new(egui::Id::unique("lumino.velocity-panel.cc-popup"))
            .order(egui::Order::Foreground)
            .fixed_pos(popup_pos)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style())
                    .fill(theme::panel_bg())
                    .stroke(Stroke::new(1.0, theme::border()))
                    .inner_margin(Margin::symmetric(2, 2))
                    .show(ui, |ui| {
                        ui.set_max_width(220.0);
                        egui::ScrollArea::vertical()
                            .max_height(220.0)
                            .show(ui, |ui| {
                                let mut options = Vec::with_capacity(129);
                                options.push(CcOption::Bend);
                                for n in 0..=127u8 {
                                    options.push(CcOption::Cc(n));
                                }
                                let selected = model.cc_selected.as_str();
                                for option in options {
                                    let label = option.to_string();
                                    let is_selected = label == selected;
                                    if ui.selectable_label(is_selected, &label).clicked() {
                                        state.action =
                                            Some(VelocityPanelAction::CcSelected(option));
                                        state.cc_open = false;
                                    }
                                }
                            });
                    });
            });
        state.popup_rect = Some(popup.response.rect);
        state.blocked.push(popup.response.rect);
    }
}

/// 点击弹层与选择器以外的空白处 / Esc 关闭，并写回 egui 临时状态。
fn close_cc_popup(ui: &egui::Ui, model: &VelocityPanelModel, state: &mut ShowState) {
    // 点击弹层与选择器以外的空白处 / Esc 关闭（与标题栏菜单、工具栏弹层同语义）。
    if model.cc_mode && state.cc_open {
        let clicked_outside = ui.input(|i| i.pointer.primary_clicked())
            && !ui.input(|i| i.pointer.interact_pos()).is_some_and(|pos| {
                state.cc_rect.is_some_and(|rect| rect.contains(pos))
                    || state.popup_rect.is_some_and(|rect| rect.contains(pos))
            });
        if clicked_outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            state.cc_open = false;
        }
    }
    if !model.cc_mode {
        state.cc_open = false;
    }
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique(CC_OPEN_ID), state.cc_open));
}

/// 数据区交互：弹层展开时整体惰性，否则按 resize / 力度 / Tempo / 自动化顺序派发。
fn handle_data_area(ui: &egui::Ui, model: &VelocityPanelModel, state: &mut ShowState) {
    // 弹层在本帧出现过 = 本帧的按压 / 滚轮属于菜单：数据区整体惰性，
    // 既避免点击穿透到曲线编辑 / resize，也避免「关菜单的同一次点击顺手加锚点」。
    // 弹层被 `Area` 约束上移后会同时压住画布与卷帘滚动条，滚动条侧由
    // `scrollbar::show_raw` 的图层判定兜底。
    let menu_drawn = state.popup_rect.is_some();
    if menu_drawn {
        // 菜单展开期间数据区仍算 egui 占用：宿主不得把空白按压转给下方编辑器。
        state.blocked.push(input::blocking_rect(model.canvas));
    }

    if !menu_drawn {
        // 顶部 resize 手柄：优先于所有数据区交互（命中区已加入 blocked）。
        if state.action.is_none() {
            state.action = input::handle_resize(ui, model);
        }

        // 力度模式 + 曲线工具：egui 接管数据区编辑（上一代 Canvas 不再收这一段）。
        if model.velocity_editing && state.action.is_none() {
            state.blocked.push(input::blocking_rect(model.canvas));
            state.action = input::handle(ui, model);
        } else if model.tempo_editing && state.action.is_none() {
            // Tempo 模式：egui 接管拖拽 / 新增 / 删除。
            state.blocked.push(input::blocking_rect(model.canvas));
            state.action = input::handle_tempo(ui, model);
        } else if model.automation_editing {
            // CC / Bend 模式：egui 接管锚点拖拽 / 新增 / 右键删除。
            state.blocked.push(input::blocking_rect(model.canvas));
            if state.action.is_none() {
                state.action = input::handle_automation(ui, model);
            }
        }
    }
}

/// 便于单测 / 探针：CC 下拉弹层是否展开。
pub fn cc_menu_open(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(egui::Id::unique(CC_OPEN_ID)))
        .unwrap_or(false)
}
