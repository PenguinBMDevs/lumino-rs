//! 左侧栏音轨行：编号 / 名称 / 独奏 `S` 的绘制与命中。
//!
//! 从 `track_list.rs` 拆出（文件长度红线 ≤600 行）。几何常量仍来自
//! `crate::layout`，与列表容器（标题 / 添加行）保持同一套口径。

use egui::{Align, Align2, Color32, CornerRadius, FontId, Layout, Rect, Sense, Vec2};

use super::super::super::SidebarTrack;
use super::super::panel_action::{self, SidebarPanelAction};
use crate::icons::{Icon, IconCache};
use crate::layout;
use crate::theme;

/// 独奏按钮命中边长（像素）：与可视化 `S` 同源，保证「看到的就是点得到的」。
const SOLO_HIT: f32 = 18.0;

/// 单个音轨行：编号列（固定 36）+ 名称列（固定起点）+ 右侧独奏。
///
/// 行由 egui **独占**点击：命中即 `SelectTrack(id)` 并计入占用区，避免 上一代
/// 的同位行按钮再处理一次（两侧选择同一个音轨时不会报错，但会造成两次重绘）。
pub(super) fn track_row(
    ui: &mut egui::Ui,
    track: &SidebarTrack,
    cache: &mut IconCache,
    solo_tip: crate::widgets::HoverTip,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), layout::TRACK_ROW_H),
        Sense::click_and_drag(),
    );

    // 行按钮内边距 10 + 行内容内边距 4。
    let inset = layout::TRACK_BUTTON_PAD_X + layout::TRACK_ROW_PAD;
    let inner = Rect::from_min_max(
        egui::pos2(rect.left() + inset, rect.top()),
        egui::pos2(rect.right() - inset, rect.bottom()),
    );

    // 独奏按钮：命中区与绘制同源（右对齐到内容右边界）。
    // 上一代 `track_item` 的 S 是 `button("S").on_press(track_solo_toggled)`；
    // 迁移到 egui 后只画了文字没接命中，点击会落到整行选择上（真机表现：S 点不动）。
    let solo_rect = Rect::from_center_size(
        egui::pos2(inner.right() - SOLO_HIT * 0.5, rect.center().y),
        Vec2::splat(SOLO_HIT),
    );
    let click_in_solo = response
        .interact_pointer_pos()
        .is_some_and(|pos| solo_rect.contains(pos));
    handle_row_clicks(ui, track, &response, rect, click_in_solo);

    let text_color = track_text_color(track);
    paint_row_base(ui, track, &response, rect);
    paint_number_column(ui, track, cache, rect, inner, text_color);
    paint_name_column(&mut *ui, track, rect, inner, text_color);
    let solo_text_pos = egui::pos2(inner.right(), rect.center().y);
    paint_solo(ui, track, solo_text_pos, solo_rect, text_color, solo_tip);
    response
}

/// 行点击 / 右键动作与命中占用登记。
fn handle_row_clicks(
    ui: &egui::Ui,
    track: &SidebarTrack,
    response: &egui::Response,
    rect: Rect,
    click_in_solo: bool,
) {
    // 兜底：即使命中判定把点击给了整行，落在 S 上时也只切独奏。
    if response.clicked() && click_in_solo {
        panel_action::push_panel_action(ui.ctx(), SidebarPanelAction::TrackSoloToggled(track.id));
    }
    // 点 S 只切独奏，不同时改选中的音轨；行点击保持原语义。
    if response.clicked() && !click_in_solo {
        panel_action::push_panel_action(ui.ctx(), SidebarPanelAction::SelectTrack(track.id));
    }
    if response.secondary_clicked() {
        // 右键由 egui 接管（原同位行按钮已不存在）。
        panel_action::push_panel_action(ui.ctx(), SidebarPanelAction::OpenTrackMenu(track.id));
    }
    // 行命中区由 egui 独占。
    // 例外：行内重命名时整行不占用，保证重命名输入框的指针操作不被拦掉。
    if !track.renaming {
        super::super::super::menu::extend_blocked(ui.ctx(), [rect]);
    }
}

/// 行底色绘制。
fn paint_row_base(ui: &egui::Ui, track: &SidebarTrack, response: &egui::Response, rect: Rect) {
    let background = row_background(track, response.hovered());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(6), background);
}

/// 第一列：编号 / 指挥轨图标，固定 36 宽，左内边距 2。
fn paint_number_column(
    ui: &egui::Ui,
    track: &SidebarTrack,
    cache: &mut IconCache,
    rect: Rect,
    inner: Rect,
    text_color: Color32,
) {
    let column_left = inner.left() + 2.0;
    if track.conductor {
        let icon_rect = Rect::from_center_size(
            egui::pos2(column_left + 9.0, rect.center().y),
            Vec2::splat(18.0),
        );
        cache.paint(ui.painter(), Icon::Clock, icon_rect);
    } else {
        ui.painter().text(
            egui::pos2(column_left, rect.center().y),
            Align2::LEFT_CENTER,
            &track.label,
            FontId::proportional(theme::font::LABEL),
            text_color,
        );
    }
}

/// 第二列：名称（重命名输入框或只读标签）。
fn paint_name_column(
    ui: &mut egui::Ui,
    track: &SidebarTrack,
    rect: Rect,
    inner: Rect,
    text_color: Color32,
) {
    // 第二列：名称。指挥轨图标列 24 宽（生产 `track_item` 的图标容器），
    // 普通轨编号列 36 宽，因此指挥轨名称起点比其它轨早 12px（与 上一代 一致）。
    let label_col = if track.conductor {
        layout::TRACK_ADD_ICON_COL_W
    } else {
        layout::TRACK_LABEL_COL_W
    };
    let name_left = inner.left() + label_col + layout::TRACK_COL_GAP;
    let name_rect = Rect::from_min_max(
        egui::pos2(name_left, rect.top()),
        egui::pos2(inner.right() - 12.0, rect.bottom()),
    );
    let mut name_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(name_rect)
            .layout(Layout::left_to_right(Align::Center)),
    );
    if track.renaming {
        paint_rename_editor(ui, &mut name_ui, track, rect, name_rect);
    } else {
        paint_name_label(ui, &mut name_ui, track, text_color);
    }
}

/// 重命名输入框：焦点抢占、显式几何与失焦收口。
fn paint_rename_editor(
    ui: &egui::Ui,
    name_ui: &mut egui::Ui,
    track: &SidebarTrack,
    rect: Rect,
    name_rect: Rect,
) {
    // 行内重命名由 egui 绘制输入框（上一代 版本会被底层画布纹理盖住）。
    let mut buffer = track.rename_buffer.clone();
    let input_id = egui::Id::unique(("lumino.track-rename", track.id));
    // 进入重命名的首帧抢一次焦点（否则按键仍落到 上一代，输入进不来）。
    // 只抢一次：用户点走之后不再往回拽。
    let focus_once = panel_action::rename_focus_once_id(track.id);
    if !ui
        .ctx()
        .data(|d| d.get_temp::<bool>(focus_once).unwrap_or(false))
    {
        ui.ctx().data_mut(|d| d.insert_temp(focus_once, true));
        name_ui.memory_mut(|m| m.request_focus(input_id));
    }
    // 显式几何，不依赖 `Layout::left_to_right(Align::Center)` 的隐式跨轴对齐：
    //
    // egui 0.37 的 `TextEditStyle` 把内容钉死在 `Align2::LEFT_TOP`（主题默认），
    // 而主题的 `interact_size.y = 24` 又让输入框至少 24 高；文字行高仅 16，
    // 多出来的 8px 全落在文字下方 → 盒子居中、文字偏上（真机「输入框偏上」）。
    //
    // 因此：矩形按行中线显式居中；高度取「行高 / 主题最小控件高」的较大者；
    // 再用 `.align(LEFT_CENTER)` 把文字在框内垂直居中。
    let font_id = FontId::proportional(theme::font::LABEL);
    let line_h = name_ui.fonts_mut(|fonts| fonts.row_height(&font_id));
    let input_h = name_ui.spacing().interact_size.y.max(line_h);
    let input_rect = Rect::from_center_size(
        egui::pos2(name_rect.center().x, rect.center().y),
        Vec2::new(name_rect.width().max(24.0), input_h),
    );
    let response = name_ui.put(
        input_rect,
        egui::TextEdit::singleline(&mut buffer)
            .id(input_id)
            .font(font_id)
            // 垂直内边距 0：框内垂直居中交给 `align`，不再靠 margin 碰运气；
            // 水平 2px 保证光标与描边不贴在一起。
            .margin(Vec2::new(2.0, 0.0))
            .align(Align2::LEFT_CENTER)
            .min_size(Vec2::new(0.0, input_h))
            .desired_width(input_rect.width()),
    );
    if response.changed() {
        panel_action::push_panel_action(
            ui.ctx(),
            SidebarPanelAction::RenameChanged {
                track_id: track.id,
                value: buffer.clone(),
            },
        );
    }
    // 重新获得焦点（用户点回输入框）→ 允许下一次失焦再上报。
    if response.gained_focus() {
        ui.ctx()
            .data_mut(|d| d.remove_temp::<bool>(panel_action::rename_blur_reported_id(track.id)));
    }
    // 失焦即收口：回车 / Tab / 点击输入框外的任意位置（画布、面板空白、
    // 工具栏、别的音轨行）都会让 TextEdit 交出焦点；Esc 由 egui 在帧首收回
    // 焦点，同样落在这里。只认「回车」会静默丢弃点击别处之前的输入。
    //
    // `lost_focus` 会连续两帧为真，用会话标记去重，保证一次失焦只上报一次。
    let blur_reported = ui.ctx().data(|d| {
        d.get_temp::<bool>(panel_action::rename_blur_reported_id(track.id))
            .unwrap_or(false)
    });
    if response.lost_focus() && !blur_reported {
        ui.ctx()
            .data_mut(|d| d.insert_temp(panel_action::rename_blur_reported_id(track.id), true));
        let outcome = if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            panel_action::RenameBlur::Cancel
        } else {
            panel_action::RenameBlur::Commit
        };
        panel_action::push_rename_blur(
            ui.ctx(),
            panel_action::SidebarRenameBlur {
                track_id: track.id,
                value: buffer,
                outcome,
            },
        );
    }
}

/// 只读名称标签。
fn paint_name_label(
    ui: &egui::Ui,
    name_ui: &mut egui::Ui,
    track: &SidebarTrack,
    text_color: Color32,
) {
    // 退出重命名：清掉会话标记（已抢焦点 / 失焦已上报），下次进入可再次聚焦。
    panel_action::reset_rename_edit_session(ui.ctx(), track.id);
    name_ui.add(
        egui::Label::new(
            egui::RichText::new(&track.name)
                .size(theme::font::LABEL)
                .color(text_color),
        )
        // egui 默认 `selectable_labels = true`：名称标签会自己吃掉指针事件，
        // 表现为「点音轨名选不中音轨」（只有点行内空白才有效）。这里显式关闭。
        .selectable(false)
        .truncate(),
    );
}

/// 独奏按钮绘制与命中（命中区在行与名称标签之后注册）。
fn paint_solo(
    ui: &egui::Ui,
    track: &SidebarTrack,
    solo_text_pos: egui::Pos2,
    solo_rect: Rect,
    text_color: Color32,
    solo_tip: crate::widgets::HoverTip,
) {
    // 独奏按钮右对齐到内容右边界（独奏时警告色）。
    ui.painter().text(
        solo_text_pos,
        Align2::RIGHT_CENTER,
        "S",
        FontId::proportional(theme::font::LABEL),
        if track.soloed {
            theme::warning()
        } else {
            text_color
        },
    );
    // 命中区在行与名称标签之后注册：egui 后注册者优先命中，
    // 避免名称 Label 与 S 的 6px 交叠把点击吃掉。
    let solo = ui.interact(
        solo_rect,
        egui::Id::unique(("lumino.track-solo", track.id)),
        Sense::click(),
    );
    let solo = crate::widgets::hover_tip(solo, solo_tip);
    if solo.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if solo.clicked() {
        panel_action::push_panel_action(ui.ctx(), SidebarPanelAction::TrackSoloToggled(track.id));
    }
}

/// 音轨文本色：自定义色按亮度取黑/白，默认主题正文色。
fn track_text_color(track: &SidebarTrack) -> Color32 {
    match track.color {
        Some(color) => {
            let luminance = 0.299 * f32::from(color.r())
                + 0.587 * f32::from(color.g())
                + 0.114 * f32::from(color.b());
            if luminance > 127.5 {
                Color32::BLACK
            } else {
                Color32::WHITE
            }
        }
        None => theme::text(),
    }
}

/// 行底色：与 上一代 `track_button_background` 同规则。
fn row_background(track: &SidebarTrack, hovered: bool) -> Color32 {
    match (track.color, track.selected, hovered) {
        (Some(color), true, _) => blend(color, theme::selected_bg(), 0.35),
        (Some(color), false, true) => blend(color, theme::toolbar_bg(), 0.25),
        (Some(color), false, false) => color,
        (None, true, _) => theme::selected_bg(),
        (None, false, true) => theme::toolbar_bg(),
        (None, false, false) => theme::base_bg(),
    }
}

/// 线性混合两个不透明颜色（`t` = 目标色占比）。
fn blend(from: Color32, to: Color32, t: f32) -> Color32 {
    let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
    Color32::from_rgb(
        mix(from.r(), to.r()),
        mix(from.g(), to.g()),
        mix(from.b(), to.b()),
    )
}
