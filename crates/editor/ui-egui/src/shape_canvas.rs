//! 形状抽屉右侧画布（egui painter 版）。
//!
//! 生产侧把文档算成一组绘制图元（线 / 三次贝塞尔 / 多边形 / 方块 / 文本）+
//! 工具栏按钮；本模块只负责原样绘制与把输入归一化成动作。
//! 所有几何以「画布局部坐标」表示（画布左上角为原点），与 iced `Canvas` 同口径。

use egui::{Color32, FontId, Pos2, Stroke};

/// 画布工具栏按钮。
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeCanvasButton {
    /// 按钮矩形（局部坐标）。
    pub rect: egui::Rect,
    /// 文案。
    pub label: String,
    /// 底色。
    pub fill: Color32,
    /// 文字色。
    pub text: Color32,
    /// 是否可点（Undo/Redo/Del 在空栈时置灰但仍绘制）。
    pub enabled: bool,
    /// 点击回传下标（0..6 工具，6=Undo，7=Redo，8=Del）。
    pub index: usize,
}

/// 直线段。
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeCanvasLine {
    /// 起点。
    pub from: Pos2,
    /// 终点。
    pub to: Pos2,
    /// 颜色。
    pub color: Color32,
    /// 线宽。
    pub width: f32,
}

/// 三次贝塞尔。
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeCanvasBezier {
    /// 控制点（p0 起点，p3 终点）。
    pub points: [Pos2; 4],
    /// 颜色。
    pub color: Color32,
    /// 线宽。
    pub width: f32,
}

/// 折线（开放路径）。
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeCanvasPoly {
    /// 顶点。
    pub points: Vec<Pos2>,
    /// 颜色。
    pub color: Color32,
    /// 线宽。
    pub width: f32,
}

/// 实心矩形。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapeCanvasRect {
    /// 矩形。
    pub rect: egui::Rect,
    /// 填充色。
    pub fill: Color32,
}

/// 实心方块（锚点 / 控制柄）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapeCanvasSquare {
    /// 中心。
    pub center: Pos2,
    /// 半边长。
    pub half: f32,
    /// 填充色。
    pub fill: Color32,
}

/// 文本。
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeCanvasText {
    /// 左上角位置。
    pub pos: Pos2,
    /// 内容。
    pub text: String,
    /// 字号。
    pub size: f32,
    /// 颜色。
    pub color: Color32,
}

/// 画布模型。
#[derive(Debug, Clone)]
pub struct ShapeCanvasModel {
    /// 画布矩形（相对窗口）。
    pub rect: egui::Rect,
    /// 画布底色。
    pub bg: Color32,
    /// 工具栏底色。
    pub toolbar_bg: Color32,
    /// 工具栏宽度（用于光标判定）。
    pub toolbar_width: f32,
    /// 工具栏按钮。
    pub buttons: Vec<ShapeCanvasButton>,
    /// 直线。
    pub lines: Vec<ShapeCanvasLine>,
    /// 贝塞尔。
    pub beziers: Vec<ShapeCanvasBezier>,
    /// 折线。
    pub polys: Vec<ShapeCanvasPoly>,
    /// 实心矩形。
    pub rects: Vec<ShapeCanvasRect>,
    /// 方块。
    pub squares: Vec<ShapeCanvasSquare>,
    /// 文本。
    pub texts: Vec<ShapeCanvasText>,
}

/// 画布产生的动作（局部坐标）。
#[derive(Debug, Clone, PartialEq)]
pub enum ShapeCanvasAction {
    /// 左键按下。
    Press {
        /// 局部坐标。
        local: Pos2,
    },
    /// 指针移动。
    Moved {
        /// 局部坐标。
        local: Pos2,
        /// 是否按住 Shift。
        shift: bool,
    },
    /// 左键松开。
    Release,
    /// 中键按下（开始平移）。
    MiddlePress {
        /// 局部坐标。
        local: Pos2,
    },
    /// 中键松开。
    MiddleRelease,
    /// 滚轮缩放。
    Scroll {
        /// 是否行模式（否则像素模式）。
        lines: bool,
        /// 是否向上。
        up: bool,
    },
    /// 工具栏按钮（按条目下标）。
    Toolbar {
        /// 0..6 工具，6=Undo，7=Redo，8=Del。
        index: usize,
    },
    /// 删除选中。
    Delete,
    /// 撤销。
    Undo,
    /// 重做。
    Redo,
    /// 分割线拖动到指定 x。
    Splitter(f32),
}

const ACTION_ID: &str = "lumino.shape-canvas.action";

/// 记录本帧动作（同一帧可能有多条：按下 + 移动 + 松开）。
pub fn push_action(ctx: &egui::Context, action: ShapeCanvasAction) {
    let mut actions = ctx
        .data(|d| d.get_temp::<Vec<ShapeCanvasAction>>(egui::Id::unique(ACTION_ID)))
        .unwrap_or_default();
    actions.push(action);
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(ACTION_ID), actions));
}

/// 取走本帧全部动作（按发生顺序）。
pub fn take_actions(ctx: &egui::Context) -> Vec<ShapeCanvasAction> {
    ctx.data_mut(|d| d.remove_temp::<Vec<ShapeCanvasAction>>(egui::Id::unique(ACTION_ID)))
        .unwrap_or_default()
}

/// 绘制画布并处理输入。
pub fn show(ui: &mut egui::Ui, model: &ShapeCanvasModel) {
    let rect = model.rect;
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(rect, 0, model.bg);
    painter.rect_filled(
        egui::Rect::from_min_size(rect.min, egui::vec2(model.toolbar_width, rect.height())),
        0,
        model.toolbar_bg,
    );

    // 图元（顺序与 iced 一致：网格 → 面积 → 描边 → 锚点 → 文本）。
    for line in &model.lines {
        painter.line_segment(
            [line.from + rect.min.to_vec2(), line.to + rect.min.to_vec2()],
            Stroke::new(line.width, line.color),
        );
    }
    for bezier in &model.beziers {
        let points = bezier.points.map(|p| p + rect.min.to_vec2());
        painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
            points,
            false,
            Color32::TRANSPARENT,
            Stroke::new(bezier.width, bezier.color),
        ));
    }
    for poly in &model.polys {
        if poly.points.len() < 2 {
            continue;
        }
        let points: Vec<Pos2> = poly
            .points
            .iter()
            .map(|p| *p + rect.min.to_vec2())
            .collect();
        painter.add(egui::Shape::line(
            points,
            Stroke::new(poly.width, poly.color),
        ));
    }
    for item in &model.rects {
        painter.rect_filled(
            egui::Rect::from_min_max(
                item.rect.min + rect.min.to_vec2(),
                item.rect.max + rect.min.to_vec2(),
            ),
            0,
            item.fill,
        );
    }
    for square in &model.squares {
        painter.rect_filled(
            egui::Rect::from_center_size(
                square.center + rect.min.to_vec2(),
                egui::vec2(square.half * 2.0, square.half * 2.0),
            ),
            0,
            square.fill,
        );
    }
    for text in &model.texts {
        painter.text(
            text.pos + rect.min.to_vec2(),
            egui::Align2::LEFT_TOP,
            &text.text,
            FontId::proportional(text.size),
            text.color,
        );
    }

    // 画布输入（先注册，工具栏按钮后注册以便压在画布之上）。
    let response = ui.interact(
        rect,
        egui::Id::unique("lumino.shape-canvas"),
        egui::Sense::click_and_drag(),
    );

    // 工具栏按钮（点击回传下标）。
    for button in &model.buttons {
        let button_rect = egui::Rect::from_min_max(
            button.rect.min + rect.min.to_vec2(),
            button.rect.max + rect.min.to_vec2(),
        );
        let response = ui.interact(
            button_rect,
            egui::Id::unique(("shape-canvas-btn", button.index)),
            egui::Sense::click(),
        );
        // 自绘按钮（与 iced 画布同口径：实心矩形 + 左上文本）。
        ui.painter().rect_filled(button_rect, 0, button.fill);
        ui.painter().text(
            button_rect.min + egui::vec2(8.0, 7.0),
            egui::Align2::LEFT_TOP,
            &button.label,
            FontId::proportional(12.0),
            button.text,
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Button, button.enabled, &button.label)
        });
        if response.clicked() && button.enabled {
            push_action(
                ui.ctx(),
                ShapeCanvasAction::Toolbar {
                    index: button.index,
                },
            );
        }
    }

    if response.hovered() {
        ui.ctx().set_cursor_icon(
            if model.toolbar_width > 0.0
                && ui
                    .ctx()
                    .pointer_latest_pos()
                    .is_some_and(|p| p.x - rect.left() <= model.toolbar_width)
            {
                egui::CursorIcon::Default
            } else {
                egui::CursorIcon::Crosshair
            },
        );
    }
    let to_local = |pos: Pos2| pos - rect.min.to_vec2();
    let shift = ui.ctx().input(|i| i.modifiers.shift);

    if response.drag_started_by(egui::PointerButton::Primary)
        && let Some(origin) = response.interact_pointer_pos()
    {
        push_action(
            ui.ctx(),
            ShapeCanvasAction::Press {
                local: to_local(origin),
            },
        );
    } else if response.clicked()
        && let Some(pos) = response.interact_pointer_pos()
    {
        // 单击（工具条 / 面积 / 曲线多段点击）也要走一次按下。
        push_action(
            ui.ctx(),
            ShapeCanvasAction::Press {
                local: to_local(pos),
            },
        );
        push_action(ui.ctx(), ShapeCanvasAction::Release);
    }
    if response.dragged_by(egui::PointerButton::Primary)
        && let Some(pos) = ui.ctx().pointer_latest_pos()
    {
        push_action(
            ui.ctx(),
            ShapeCanvasAction::Moved {
                local: to_local(pos),
                shift,
            },
        );
    }
    if response.drag_stopped_by(egui::PointerButton::Primary) {
        push_action(ui.ctx(), ShapeCanvasAction::Release);
    }
    if response.drag_started_by(egui::PointerButton::Middle)
        && let Some(pos) = response.interact_pointer_pos()
    {
        push_action(
            ui.ctx(),
            ShapeCanvasAction::MiddlePress {
                local: to_local(pos),
            },
        );
    }
    if response.dragged_by(egui::PointerButton::Middle)
        && let Some(pos) = ui.ctx().pointer_latest_pos()
    {
        push_action(
            ui.ctx(),
            ShapeCanvasAction::Moved {
                local: to_local(pos),
                shift,
            },
        );
    }
    if response.drag_stopped_by(egui::PointerButton::Middle) {
        push_action(ui.ctx(), ShapeCanvasAction::MiddleRelease);
    }
    if response.hovered() {
        let dy = ui.ctx().input(|i| i.smooth_scroll_delta.y);
        if dy.abs() > 0.0 {
            // egui 的行模式滚轮是 50 的倍数；其余按像素处理（与 iced 的两档一致）。
            let lines = dy.abs() >= 20.0 && (dy / 50.0).fract().abs() < 1e-3;
            push_action(
                ui.ctx(),
                ShapeCanvasAction::Scroll {
                    lines,
                    up: dy > 0.0,
                },
            );
        }
        if ui.ctx().input(|i| i.key_pressed(egui::Key::Delete)) {
            push_action(ui.ctx(), ShapeCanvasAction::Delete);
        }
        let (ctrl, z, y) = ui.ctx().input(|i| {
            (
                i.modifiers.ctrl || i.modifiers.command,
                i.key_pressed(egui::Key::Z),
                i.key_pressed(egui::Key::Y),
            )
        });
        if ctrl && z {
            push_action(ui.ctx(), ShapeCanvasAction::Undo);
        }
        if ctrl && y {
            push_action(ui.ctx(), ShapeCanvasAction::Redo);
        }
    }
}

/// 绘制面板与画布之间的分割线（6px），拖动回传目标宽度。
pub fn show_splitter(ui: &mut egui::Ui, rect: egui::Rect, fill: Color32) {
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    ui.painter().rect_filled(rect, 0, fill);
    let response = ui.interact(
        rect,
        egui::Id::unique("lumino.shape-splitter"),
        egui::Sense::click_and_drag(),
    );
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    if response.dragged()
        && let Some(pos) = ui.ctx().pointer_latest_pos()
    {
        push_action(ui.ctx(), ShapeCanvasAction::Splitter(pos.x));
    }
}

impl Default for ShapeCanvasModel {
    fn default() -> Self {
        Self {
            rect: egui::Rect::NOTHING,
            bg: Color32::BLACK,
            toolbar_bg: Color32::BLACK,
            toolbar_width: 0.0,
            buttons: Vec::new(),
            lines: Vec::new(),
            beziers: Vec::new(),
            polys: Vec::new(),
            rects: Vec::new(),
            squares: Vec::new(),
            texts: Vec::new(),
        }
    }
}
