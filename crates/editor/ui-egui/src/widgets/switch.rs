//! 开关控件（switch）：布尔偏好的右对齐控件，替代勾选框。
//!
//! 几何：轨道 36×18、圆角 9、圆点直径 14、内缩 2；标签左对齐、开关贴行右缘。
//! 行高沿用原勾选框的 22，布局零位移（调用方只需给出整行矩形）。
//!
//! 动画：`Context::animate_bool_with_time`（120ms 线性）。egui 在动画进行中会
//! 自动 `request_repaint`，不依赖外部重绘调度；`AnimationManager` 首帧直接落到
//! 目标值，所以页面 / 弹窗打开时不会“滑一下”。
//!
//! 语义：在无障碍树里仍以 `Role::CheckBox` + `selected` 暴露（egui 没有 Switch
//! 角色），标签文本完整上报（截断只影响绘制），`kittest` 的 `get_by_label`
//! 依赖它。

use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Response, Sense, Stroke, Vec2};

/// 轨道宽度（逻辑像素）。
pub const TRACK_W: f32 = 36.0;
/// 轨道高度（逻辑像素）。
pub const TRACK_H: f32 = 18.0;
/// 圆点半径。
const KNOB_R: f32 = 7.0;
/// 圆点与轨道的内缩。
const INSET: f32 = 2.0;
/// 状态切换动画时长（秒）。
const ANIM_SECS: f32 = 0.12;
/// 标签与轨道的最小间距。
const LABEL_GAP: f32 = 8.0;
/// 标签命中区高度（与原勾选框一致）。
const LABEL_H: f32 = 22.0;

/// 开关轨道 / 圆点配色。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwitchColors {
    /// 开启态轨道填充。
    pub on: Color32,
    /// 关闭态轨道填充。
    pub off: Color32,
    /// 轨道边框。
    pub border: Color32,
    /// 开启态圆点。
    pub knob_on: Color32,
    /// 关闭态圆点。
    pub knob_off: Color32,
    /// 禁用态圆点。
    pub knob_disabled: Color32,
    /// 禁用态轨道填充。
    pub disabled: Color32,
}

/// 一行开关的文案与配色。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwitchStyle {
    /// 标签颜色。
    pub label: Color32,
    /// 禁用态标签颜色。
    pub label_dim: Color32,
    /// 标签字号。
    pub label_size: f32,
    /// 轨道 / 圆点配色。
    pub colors: SwitchColors,
}

/// 一行开关的数据。
#[derive(Debug, Clone, Copy)]
pub struct SwitchRow<'a> {
    /// 稳定 id（跨帧不变；交互与动画都依赖它）。
    pub id: egui::Id,
    /// 当前状态。
    pub checked: bool,
    /// 是否可交互（`false` 时只保留悬浮说明）。
    pub enabled: bool,
    /// 标签（无障碍树里上报完整文本）。
    pub label: &'a str,
    /// 悬浮说明（空 = 不挂说明）。
    pub hint: &'a str,
}

/// 一行开关的绘制结果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwitchHit {
    /// 轨道矩形（命中区注册 / 测试点击用）。
    pub track: Rect,
    /// 本帧是否被点击（调用方据此回传动作）。
    pub toggled: bool,
}

/// 行内右对齐、垂直居中的轨道矩形。
pub fn track_rect(row: Rect) -> Rect {
    Rect::from_min_size(
        Pos2::new(row.right() - TRACK_W, row.center().y - TRACK_H * 0.5),
        Vec2::new(TRACK_W, TRACK_H),
    )
}

/// 绘制一行「标签（左）+ 开关（右）」。
///
/// 标签与轨道都可点（与旧勾选框一致：点文字同样切换）；`enabled = false` 时
/// 两者都只留悬浮说明。
pub fn switch_row(
    ui: &mut egui::Ui,
    row: Rect,
    spec: &SwitchRow<'_>,
    style: &SwitchStyle,
) -> SwitchHit {
    let track = track_rect(row);
    let mut toggled = false;

    let response = switch_response(ui, track, spec);
    if spec.enabled && response.clicked() {
        toggled = true;
    }

    // 标签：按轨道左侧可用宽度截断（绘制截断、无障碍文本完整）。
    let label_max = (track.left() - LABEL_GAP - row.left()).max(1.0);
    let label = crate::widgets::truncate_to_width(ui, spec.label, style.label_size, label_max);
    let label_w = crate::widgets::text_width(ui, &label, style.label_size);
    let label_rect = Rect::from_min_size(
        Pos2::new(row.left(), row.center().y - LABEL_H * 0.5),
        Vec2::new(label_w.max(1.0), LABEL_H),
    );
    let label_response = ui.interact(
        label_rect,
        spec.id.with("label"),
        if spec.enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    if spec.enabled && label_response.clicked() {
        toggled = true;
    }
    if !spec.hint.is_empty() {
        label_response.on_hover_text(spec.hint);
    }

    paint(ui, track, spec, style);
    ui.painter().text(
        Pos2::new(row.left(), row.center().y),
        Align2::LEFT_CENTER,
        &label,
        FontId::proportional(style.label_size),
        if spec.enabled {
            style.label
        } else {
            style.label_dim
        },
    );

    SwitchHit { track, toggled }
}

/// 轨道命中区 + 无障碍信息（`Role::CheckBox` 保持屏幕阅读器与 kittest 口径）。
fn switch_response(ui: &mut egui::Ui, track: Rect, spec: &SwitchRow<'_>) -> Response {
    let response = ui.interact(
        track,
        spec.id,
        if spec.enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::CheckBox, spec.enabled, spec.checked, spec.label)
    });
    if spec.enabled {
        return response.on_hover_cursor(egui::CursorIcon::PointingHand);
    }
    response
}

/// 轨道 + 圆点：状态用 alpha 叠加插值，避免依赖颜色空间的混合 API。
fn paint(ui: &egui::Ui, track: Rect, spec: &SwitchRow<'_>, style: &SwitchStyle) {
    // 动画值：0 = 关、1 = 开。禁用时不做动画，直接落到目标态。
    let t = if spec.enabled {
        ui.ctx()
            .animate_bool_with_time(spec.id.with("anim"), spec.checked, ANIM_SECS)
    } else if spec.checked {
        1.0
    } else {
        0.0
    };
    let radius = CornerRadius::same((TRACK_H * 0.5) as u8);
    // 禁用态用中性底：不伪装成可点，但圆点仍停在真实状态一侧。
    let (fill, accent_alpha) = if spec.enabled {
        (style.colors.off, t)
    } else {
        (style.colors.disabled, 0.0)
    };
    ui.painter().rect(
        track,
        radius,
        fill,
        Stroke::new(1.0, style.colors.border),
        egui::StrokeKind::Inside,
    );
    if accent_alpha > 0.0 {
        ui.painter()
            .rect_filled(track, radius, with_alpha(style.colors.on, accent_alpha));
    }

    let travel = TRACK_W - 2.0 * (INSET + KNOB_R);
    let center = Pos2::new(track.left() + INSET + KNOB_R + travel * t, track.center().y);
    // 禁用态圆点也压暗一档：光靠标签变暗不足以表达“不可操作”。
    let knob = if spec.enabled {
        style.colors.knob_off
    } else {
        style.colors.knob_disabled
    };
    ui.painter().circle_filled(center, KNOB_R, knob);
    if spec.enabled && t > 0.0 {
        ui.painter()
            .circle_filled(center, KNOB_R, with_alpha(style.colors.knob_on, t));
    }
}

/// 给颜色叠加 alpha（`t` 会被夹到 0..=1）。
fn with_alpha(color: Color32, t: f32) -> Color32 {
    let alpha = (255.0 * t.clamp(0.0, 1.0)).round() as u8;
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_sits_at_row_right_edge() {
        let row = Rect::from_min_size(Pos2::new(10.0, 100.0), Vec2::new(300.0, 22.0));
        let track = track_rect(row);
        assert_eq!(track.width(), TRACK_W);
        assert_eq!(track.height(), TRACK_H);
        assert_eq!(track.right(), row.right(), "轨道必须贴行右缘");
        assert_eq!(track.center().y, row.center().y, "轨道必须垂直居中");
        assert_eq!(track.left(), row.right() - TRACK_W);
    }

    #[test]
    fn alpha_blend_clamps_and_scales() {
        let color = Color32::from_rgb(0x2A, 0xC3, 0xDE);
        assert_eq!(with_alpha(color, -1.0).a(), 0);
        assert_eq!(with_alpha(color, 0.0).a(), 0);
        assert_eq!(with_alpha(color, 1.0).a(), 255);
        let half = with_alpha(color, 0.5);
        assert!(
            (120..=136).contains(&half.a()),
            "半程 alpha 应在 128 附近，实际 {}",
            half.a()
        );
    }
}
