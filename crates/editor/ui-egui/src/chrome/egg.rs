//! 「关于」页 logo 彩蛋飞行层（UI-007）。
//!
//! 状态机在 `lumino_ui_core::state::about_egg`（纯几何，逻辑像素）；这里只负责
//! 按 [`EggModel`] 把 logo 以旋转 + 透明度画到窗口坐标系里。
//!
//! 旧版 版本用的是「固定方盒 + 居中」占位法（`column![space(y), row![space(x), slot]]`）；
//! egui 直接用 painter 在任意位置画旋转网格，不再需要占位控件，也不会捕获鼠标。

use egui::Color32;

use crate::icons::{Icon, IconCache};

/// 飞行中的 logo 快照（窗口逻辑像素）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EggModel {
    /// 方盒左上角 x（与生产 `AboutEggState::position` 同坐标）。
    pub x: f32,
    /// 方盒左上角 y。
    pub y: f32,
    /// 方盒边长（`ABOUT_LOGO_BOX`）。
    pub box_size: f32,
    /// logo 绘制边长（`ABOUT_LOGO_SIZE × spin_fit_scale`）。
    pub icon_size: f32,
    /// 旋转角（弧度）。
    pub angle: f32,
    /// 不透明度（0..=1）。
    pub opacity: f32,
}

/// 画飞行中的 logo（`full` = 整窗逻辑矩形）。
pub fn show(ui: &egui::Ui, full: egui::Rect, model: &EggModel, icons: &mut IconCache) {
    if model.opacity <= 0.001 || model.icon_size <= 0.0 {
        return;
    }
    let texture = icons.texture(ui.ctx(), Icon::LogoInApp, model.icon_size, None);
    let size = texture.size_vec2();
    // 方盒左上角 → 方盒中心（logo 在方盒内居中，与 旧版 的 slot 对齐一致）。
    let center = egui::pos2(
        full.left() + model.x + model.box_size * 0.5,
        full.top() + model.y + model.box_size * 0.5,
    );
    let half = size * 0.5;
    let (sin, cos) = model.angle.sin_cos();
    let corner = |dx: f32, dy: f32| {
        egui::pos2(
            center.x + dx * cos - dy * sin,
            center.y + dx * sin + dy * cos,
        )
    };
    let color = Color32::WHITE.gamma_multiply(model.opacity.clamp(0.0, 1.0));
    let mut mesh = egui::Mesh::with_texture(texture.id());
    mesh.vertices = vec![
        egui::epaint::Vertex {
            pos: corner(-half.x, -half.y),
            uv: egui::pos2(0.0, 0.0),
            color,
        },
        egui::epaint::Vertex {
            pos: corner(half.x, -half.y),
            uv: egui::pos2(1.0, 0.0),
            color,
        },
        egui::epaint::Vertex {
            pos: corner(half.x, half.y),
            uv: egui::pos2(1.0, 1.0),
            color,
        },
        egui::epaint::Vertex {
            pos: corner(-half.x, half.y),
            uv: egui::pos2(0.0, 1.0),
            color,
        },
    ];
    mesh.indices = vec![0, 1, 2, 0, 2, 3];
    ui.painter().add(egui::Shape::mesh(mesh));
    // 飞行层不捕获鼠标（与 旧版 版「无 mouse_area」一致）。
}
