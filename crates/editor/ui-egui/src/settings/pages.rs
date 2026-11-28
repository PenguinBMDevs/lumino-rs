//! 设置「快捷键 / 洋葱皮 / 兼容性」三页。
//!
//! 全部为只读模型 + 动作回传；布局按生产 `ui-settings` 的页面结构复刻：
//! 内容内边距 20、标题 18、正文 14、辅助提示 12，元素间距 10。

use egui::{Align, Align2, Color32, CornerRadius, FontId, Layout, Pos2, Rect, Sense, Stroke, Vec2};

use super::super::chrome::{
    CompatibilityPageModel, OnionSkinPageModel, SettingsPalette, ShortcutsPageModel,
};
use super::super::{theme, widgets};
use super::action::{self, SettingsAction};
use super::{SettingsState, set_hits};
use crate::widgets::text_width;

/// 正文 / 标题 / 提示字号（与生产常量一致）。
const CONTENT: f32 = 14.0;
const HINT: f32 = 12.0;
/// 内容内边距。
const PAD: f32 = 20.0;
/// 元素间距。
const GAP: f32 = 10.0;

pub(super) fn title(ui: &egui::Ui, x: f32, base: f32, text: &str, palette: &SettingsPalette) {
    let max = (ui.max_rect().right() - PAD - x).max(1.0);
    crate::text::paint_truncated(
        ui,
        Pos2::new(x, base + PAD + 10.0),
        Align2::LEFT_CENTER,
        text,
        theme::font::TITLE,
        palette.text,
        max,
    );
}

/// 快捷键页：标题 + 占位说明。
pub(super) fn shortcuts(
    ui: &mut egui::Ui,
    rect: Rect,
    model: &ShortcutsPageModel,
    palette: &SettingsPalette,
) {
    let x = rect.left() + PAD;
    let base = rect.top();
    title(ui, x, base, &model.title, palette);
    // 占位说明属于解释性长文本：按卡片可用宽度折行，不硬画单行。
    let wrap = (rect.right() - PAD - x).max(1.0);
    crate::text::paint_wrapped(
        ui,
        Pos2::new(x, base + 75.0 - crate::text::line_height(ui, CONTENT) * 0.5),
        &model.placeholder,
        CONTENT,
        palette.text_dim,
        wrap,
    );
}

/// 洋葱皮页：渲染后端下拉（加载 MIDI 后只读）+ 能力边界提示。
pub(super) fn onion_skin(
    ui: &mut egui::Ui,
    rect: Rect,
    model: &OnionSkinPageModel,
    palette: &SettingsPalette,
) {
    let x = rect.left() + PAD;
    let base = rect.top();
    title(ui, x, base, &model.title, palette);

    let row_center = base + 77.0;
    let wrap = (rect.right() - PAD - x).max(1.0);
    let label_font = FontId::proportional(CONTENT);
    // 右侧控件占位固定：标签必须截断，不能压到下拉框 / 锁定值上。
    let reserve = if model.locked {
        crate::text::text_width(ui, &model.locked_value, CONTENT) + 8.0
    } else {
        120.0 + 8.0
    };
    let label =
        crate::text::truncate_to_width(ui, &model.label, CONTENT, (wrap - 32.0 - reserve).max(1.0));
    ui.painter().text(
        Pos2::new(x, row_center),
        Align2::LEFT_CENTER,
        &label,
        label_font.clone(),
        palette.text,
    );
    let label_w = ui
        .painter()
        .layout_no_wrap(label, label_font, palette.text)
        .size()
        .x;
    // 生产 `row![label, space(16), pick_list].spacing(8)` → 标签与控件间 32。
    let field_x = x + label_w + 32.0;
    let below = backend_control(
        ui,
        rect,
        RowAnchor {
            x,
            field_x,
            row_center,
            wrap,
        },
        base + 77.0 + 32.0 + GAP + 6.0,
        model,
        palette,
    );

    crate::text::paint_wrapped(
        ui,
        Pos2::new(x, below - crate::text::line_height(ui, HINT) * 0.5),
        &model.hint,
        HINT,
        palette.text_dim,
        wrap,
    );
}

/// 洋葱皮后端行的绘制锚点（标签列起点 / 控件起点 / 行中心 / 折行宽度）。
#[derive(Debug, Clone, Copy)]
struct RowAnchor {
    x: f32,
    field_x: f32,
    row_center: f32,
    wrap: f32,
}

/// 后端控件：锁定态只读值 + 原因，未锁定态下拉框；返回能力提示的起点 y。
fn backend_control(
    ui: &mut egui::Ui,
    rect: Rect,
    anchor: RowAnchor,
    mut below: f32,
    model: &OnionSkinPageModel,
    palette: &SettingsPalette,
) -> f32 {
    let RowAnchor {
        x,
        field_x,
        row_center,
        wrap,
    } = anchor;

    if model.locked {
        crate::text::paint_truncated(
            ui,
            Pos2::new(field_x, row_center),
            Align2::LEFT_CENTER,
            &model.locked_value,
            CONTENT,
            palette.text,
            (rect.right() - PAD - field_x).max(1.0),
        );
        // 锁定原因属于解释性文本：另起一行折行，而不是挤在行尾。
        if !model.locked_hint.is_empty() {
            let h = crate::text::paint_wrapped(
                ui,
                Pos2::new(x, row_center + 16.0 + GAP),
                &model.locked_hint,
                HINT,
                palette.text_dim,
                wrap,
            );
            below = row_center + 16.0 + GAP + h + GAP;
        }
    } else {
        let combo = Rect::from_min_size(
            Pos2::new(field_x, row_center - 16.0),
            Vec2::new(120.0, 32.0),
        );
        let mut hits = super::hits(ui.ctx());
        hits.onion_combo = Some(combo);
        set_hits(ui.ctx(), hits);
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(combo)
                .layout(Layout::top_down(Align::Min)),
        );
        let selected = model
            .options
            .get(model.selected)
            .cloned()
            .unwrap_or_default();
        if let Some(index) = widgets::combo_box(
            &mut child,
            model.label.as_str(),
            &selected,
            &model.options,
            120.0,
            palette,
        ) {
            action::push_action(
                ui.ctx(),
                SettingsAction::OnionSkinBackend(if index == 0 {
                    lumino_core::storage::config::OnionSkinBackend::Gpu
                } else {
                    lumino_core::storage::config::OnionSkinBackend::Cpu
                }),
            );
        }
    }

    below
}

/// 兼容性页：检查 / 复制按钮、结果区、三个开关。
pub(super) fn compatibility(
    ui: &mut egui::Ui,
    rect: Rect,
    model: &CompatibilityPageModel,
    palette: &SettingsPalette,
) {
    let x = rect.left() + PAD;
    let base = rect.top();
    title(ui, x, base, &model.title, palette);

    // 按钮行。
    let button_y = base + PAD + 21.0 + 20.0;
    let content_w = (rect.width() - PAD * 2.0).max(1.0);
    let check_w = (text_width(ui, &model.check_label, CONTENT) + 24.0).min(content_w * 0.48);
    let check = Rect::from_min_size(Pos2::new(x, button_y), Vec2::new(check_w, 30.0));
    let mut hits = super::hits(ui.ctx());
    hits.compat_check = Some(check);
    set_hits(ui.ctx(), hits);
    if action_button(ui, check, &model.check_label, model.check_enabled, palette).clicked() {
        action::push_action(ui.ctx(), SettingsAction::RunGpuCheck);
    }
    let copy_x = check.right() + 12.0;
    let copy_w = (text_width(ui, &model.copy_label, CONTENT) + 24.0)
        .min((rect.right() - PAD - copy_x).max(1.0));
    let copy = Rect::from_min_size(Pos2::new(copy_x, button_y), Vec2::new(copy_w, 30.0));
    let mut hits = super::hits(ui.ctx());
    hits.compat_copy = Some(copy);
    set_hits(ui.ctx(), hits);
    if action_button(ui, copy, &model.copy_label, model.copy_enabled, palette).clicked() {
        action::push_action(ui.ctx(), SettingsAction::CopyGpuDiagnostics);
    }

    // 结果区：按卡片宽度换行（生产 上一代 的 `text` 会自动换行，不能单行硬画）。
    let wrap = (rect.right() - PAD - x).max(80.0);
    let mut y = button_y + 30.0 + 10.0;
    for line in &model.result_lines {
        let size = if line.dim { HINT } else { CONTENT };
        let color = if line.dim {
            palette.text_dim
        } else {
            palette.text
        };
        let galley =
            ui.painter()
                .layout(line.text.clone(), FontId::proportional(size), color, wrap);
        let height = galley.size().y;
        ui.painter().galley(Pos2::new(x, y), galley, color);
        y += height + 2.0;
    }

    // 三个开关（间距 20 起、行间步进沿用旧勾选框 32）。
    y += 20.0;
    let style = switch_style(palette);
    let row_width = (rect.right() - PAD - x).max(crate::widgets::TRACK_W);
    let row_at = |y: f32| Rect::from_min_size(Pos2::new(x, y), Vec2::new(row_width, 22.0));
    y = compat_switch(
        ui,
        row_at(y),
        &crate::widgets::SwitchRow {
            id: egui::Id::unique("lumino.settings.switch.check-on-startup"),
            checked: model.check_on_startup,
            enabled: true,
            label: model.check_on_startup_label.as_str(),
            hint: model.check_on_startup_hint.as_str(),
        },
        &style,
        SettingsAction::GpuCheckOnStartup,
    );
    y = compat_switch(
        ui,
        row_at(y),
        &crate::widgets::SwitchRow {
            id: egui::Id::unique("lumino.settings.switch.warning"),
            checked: model.show_warning,
            enabled: true,
            label: model.warning_label.as_str(),
            hint: model.warning_hint.as_str(),
        },
        &style,
        |visible| SettingsAction::GpuWarningSuppressed(!visible),
    );
    compat_switch(
        ui,
        row_at(y),
        &crate::widgets::SwitchRow {
            id: egui::Id::unique("lumino.settings.switch.domino"),
            checked: model.domino,
            enabled: model.domino_enabled,
            label: model.domino_label.as_str(),
            hint: model.domino_hint.as_str(),
        },
        &style,
        SettingsAction::DominoClipboard,
    );
}

/// 设置对话框的开关样式（文案 + 轨道配色）。
pub(super) fn switch_style(palette: &SettingsPalette) -> crate::widgets::SwitchStyle {
    crate::widgets::SwitchStyle {
        label: palette.text,
        label_dim: palette.text_dim,
        label_size: CONTENT,
        colors: crate::widgets::SwitchColors {
            on: palette.accent,
            off: palette.toolbar,
            border: palette.input_border,
            // 开启态圆点沿用旧勾选框的勾线墨色：深浅主题下都压在强调色上可读。
            knob_on: Color32::from_rgb(0x12, 0x1A, 0x2A),
            knob_off: palette.text,
            knob_disabled: palette.text_dim,
            disabled: palette.border,
        },
    }
}

/// 页内按钮：`background.weak` 底 + 1px 边框；禁用态文字变暗。
pub(super) fn action_button(
    ui: &mut egui::Ui,
    rect: Rect,
    label: &str,
    enabled: bool,
    palette: &SettingsPalette,
) -> egui::Response {
    let response = ui.interact(
        rect,
        egui::Id::unique(("lumino.settings.button", label)),
        Sense::click(),
    );
    let fill = if response.hovered() && enabled {
        Color32::from_white_alpha(20)
    } else {
        palette.toolbar
    };
    ui.painter().rect(
        rect,
        CornerRadius::same(4),
        fill,
        Stroke::new(1.0, palette.input_border),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        crate::text::truncate_to_width(ui, label, CONTENT, (rect.width() - 12.0).max(1.0)),
        FontId::proportional(CONTENT),
        if enabled {
            palette.text
        } else {
            palette.text_dim
        },
    );
    if !enabled {
        // 禁用：吞掉点击（返回的 response 仍会 clicked，调用方据此不再发送动作）。
        return response.interact(Sense::hover());
    }
    response
}

/// 兼容性页的一行开关：绘制 + 注册命中区，返回下一行 y。
fn compat_switch(
    ui: &mut egui::Ui,
    row: Rect,
    spec: &crate::widgets::SwitchRow<'_>,
    style: &crate::widgets::SwitchStyle,
    action: impl Fn(bool) -> SettingsAction,
) -> f32 {
    let hit = crate::widgets::switch_row(ui, row, spec, style);
    let mut hits = super::hits(ui.ctx());
    hits.compat_checks.push((spec.label.to_owned(), hit.track));
    set_hits(ui.ctx(), hits);
    if hit.toggled {
        action::push_action(ui.ctx(), action(!spec.checked));
    }
    row.top() + 32.0
}

/// 少数页面共用：当前页没有任何表单（保留给后续页面）。
#[allow(dead_code)]
pub(super) fn placeholder(
    ui: &mut egui::Ui,
    rect: Rect,
    state: &SettingsState,
    palette: &SettingsPalette,
) {
    let x = rect.left() + PAD;
    let base = rect.top();
    title(ui, x, base, &state.labels.title, palette);
}
