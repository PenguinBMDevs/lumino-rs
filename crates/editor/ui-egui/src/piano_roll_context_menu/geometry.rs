//! 右键菜单的单竖列几何：分组、面板放置、行矩形与滚动钳位。

use egui::{FontId, Pos2, Rect, Vec2};
use lumino_message::ContextMenuTarget;
use lumino_message::context_menu::{item_label, menu_groups};

use super::{
    ICON, ICON_GAP, OFFSET, PADDING, PianoRollMenuModel, ROW_H, ROW_PAD, SEPARATOR_H, TEXT,
    TEXT_SIZE,
};

/// 单列内一个分组的几何（内容区坐标系，不含滚动）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct GroupLayout {
    /// 组内首行的全局行号。
    pub(super) first_row: usize,
    /// 组内行数。
    pub(super) rows: usize,
    /// 组内容顶部相对内容区顶部的偏移（已含前面的分隔线）。
    pub(super) top: f32,
}

/// 本帧菜单几何（窗口坐标）。
#[derive(Debug, Clone, PartialEq)]
pub struct MenuLayout {
    /// 面板矩形（高度已钳到可用区）。
    pub panel: Rect,
    /// 内容视口（面板内边距之内）。
    pub(super) content: Rect,
    /// 单列行宽。
    pub(super) width: f32,
    /// 内容总高（含组间分隔线，不含内边距）。
    pub(super) content_height: f32,
    /// 当前滚动偏移。
    pub(super) scroll: f32,
    /// 分组（单列内自上而下）。
    pub(super) groups: Vec<GroupLayout>,
}

impl MenuLayout {
    /// 最大滚动偏移（内容不超出视口时为 0）。
    pub fn max_scroll(&self) -> f32 {
        (self.content_height - self.content.height()).max(0.0)
    }

    /// 当前滚动偏移。
    pub fn scroll(&self) -> f32 {
        self.scroll
    }

    /// 单列总行数。
    pub fn row_count(&self) -> usize {
        self.groups
            .last()
            .map_or(0, |group| group.first_row + group.rows)
    }

    pub(super) fn set_scroll(&mut self, scroll: f32) {
        self.scroll = scroll.clamp(0.0, self.max_scroll());
    }

    /// 某行顶部相对内容区顶部的偏移（不含滚动）。
    pub(super) fn row_top(&self, row: usize) -> Option<f32> {
        let group = self
            .groups
            .iter()
            .find(|group| row >= group.first_row && row < group.first_row + group.rows)?;
        Some(group.top + (row - group.first_row) as f32 * ROW_H)
    }

    /// 滚动到 `offset` 时视口顶端所在的行（滚轮滚动后据此续接键盘焦点）。
    pub(super) fn row_at(&self, offset: f32) -> usize {
        let mut row = 0;
        for group in &self.groups {
            if group.top > offset {
                break;
            }
            let within = ((offset - group.top) / ROW_H).floor().max(0.0) as usize;
            row = group.first_row + within.min(group.rows.saturating_sub(1));
        }
        row.min(self.row_count().saturating_sub(1))
    }
}

/// 按行宽与分组行数放置面板（纯几何，可在无 `egui::Context` 的测试里直接调用）。
///
/// `width` 是单列行宽，`rows` 是各组行数，`scroll` 为期望滚动偏移（会被钳位）。
pub fn place(model: &PianoRollMenuModel, width: f32, rows: &[usize], scroll: f32) -> MenuLayout {
    let row_height: f32 = rows.iter().sum::<usize>() as f32 * ROW_H;
    let content_height = row_height + rows.len().saturating_sub(1) as f32 * SEPARATOR_H;
    let panel_w = width + PADDING * 2.0;
    let panel_h = (content_height + PADDING * 2.0)
        .min(model.available.height())
        .max(0.0);

    let x = (model.position.x + OFFSET).clamp(
        model.available.left(),
        (model.available.right() - panel_w - OFFSET).max(model.available.left()),
    );
    let below = model.position.y + OFFSET;
    let y = if below + panel_h > model.available.bottom() {
        (model.position.y - panel_h - OFFSET).max(model.available.top())
    } else {
        below
    };
    let y = y.min((model.available.bottom() - panel_h).max(model.available.top()));
    let panel = Rect::from_min_size(egui::pos2(x, y), Vec2::new(panel_w, panel_h));

    let viewport_h = (panel_h - PADDING * 2.0).max(0.0);
    let content = Rect::from_min_size(
        egui::pos2(panel.left() + PADDING, panel.top() + PADDING),
        Vec2::new(width, viewport_h),
    );
    let max_scroll = (content_height - viewport_h).max(0.0);

    let mut groups = Vec::with_capacity(rows.len());
    let mut first_row = 0;
    let mut top = 0.0;
    for (index, count) in rows.iter().copied().enumerate() {
        if index > 0 {
            top += SEPARATOR_H;
        }
        groups.push(GroupLayout {
            first_row,
            rows: count,
            top,
        });
        first_row += count;
        top += count as f32 * ROW_H;
    }

    MenuLayout {
        panel,
        content,
        width,
        content_height,
        scroll: scroll.clamp(0.0, max_scroll),
        groups,
    }
}

/// 第 `row` 行的矩形（窗口坐标，未裁剪到视口）；越界返回 [`Rect::NOTHING`]。
pub fn item_rect(layout: &MenuLayout, row: usize) -> Rect {
    let Some(top) = layout.row_top(row) else {
        return Rect::NOTHING;
    };
    Rect::from_min_size(
        Pos2::new(
            layout.content.left(),
            layout.content.top() + top - layout.scroll,
        ),
        Vec2::new(layout.width, ROW_H),
    )
}

/// 按当前字体实测单列行宽（文字按 galley 度量，CJK / 拉丁都不截断）。
fn column_width(ui: &egui::Ui, target: ContextMenuTarget) -> f32 {
    let mut text = 0.0_f32;
    for group in menu_groups(target) {
        for item in *group {
            let galley = ui.painter().layout_no_wrap(
                item_label(*item).to_owned(),
                FontId::proportional(TEXT_SIZE),
                TEXT,
            );
            text = text.max(galley.size().x);
        }
    }
    ROW_PAD * 2.0 + ICON + ICON_GAP + text
}

/// 计算本帧布局（需要 `ui` 来实测文字宽度）。
pub fn layout(ui: &egui::Ui, model: &PianoRollMenuModel, scroll: f32) -> MenuLayout {
    let rows: Vec<usize> = menu_groups(model.target)
        .iter()
        .map(|group| group.len())
        .collect();
    let width = column_width(ui, model.target);
    place(model, width, &rows, scroll)
}
