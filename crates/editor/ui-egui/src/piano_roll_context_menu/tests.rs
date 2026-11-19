//! 菜单几何单测（单竖列：分组 / 分隔线 / 滚动）

use super::*;

fn model(target: ContextMenuTarget) -> PianoRollMenuModel {
    PianoRollMenuModel {
        position: Pos2::new(400.0, 300.0),
        available: Rect::from_min_size(Pos2::new(248.0, 102.0), Vec2::new(1200.0, 800.0)),
        target,
    }
}

fn rows(target: ContextMenuTarget) -> Vec<usize> {
    menu_groups(target)
        .iter()
        .map(|items| items.len())
        .collect()
}

/// 用固定列宽构造布局，避免单测依赖字体度量。
fn sample(target: ContextMenuTarget) -> MenuLayout {
    place(&model(target), 160.0, &rows(target), 0.0)
}

#[test]
fn panel_fits_inside_available_and_flips_up() {
    let rows = [8_usize, 2];

    // 靠右下角：下方放不下 → 翻到上方，且不越界。
    let mut m = model(ContextMenuTarget::Notes);
    m.position = Pos2::new(1400.0, 880.0);
    let layout = place(&m, 160.0, &rows, 0.0);
    assert!(
        m.available.contains_rect(layout.panel),
        "{:?} 越界",
        layout.panel
    );
    assert!(layout.panel.bottom() <= m.position.y, "应翻到鼠标上方");

    // 靠左上角：正常向下展开。
    let mut m = model(ContextMenuTarget::Notes);
    m.position = Pos2::new(260.0, 110.0);
    let layout = place(&m, 160.0, &rows, 0.0);
    assert!(layout.panel.top() >= m.position.y, "应向下展开");
    assert!(m.available.contains_rect(layout.panel));
}

#[test]
fn panel_size_follows_rows_and_single_column() {
    let notes = sample(ContextMenuTarget::Notes);
    let drawn = sample(ContextMenuTarget::DrawnShape);
    let pending = sample(ContextMenuTarget::PendingDrawnShape);

    // 高度 = 行数 × 24 + 组间分隔线 + 上下内边距
    assert_eq!(notes.panel.height(), 10.0 * 24.0 + 9.0 + 16.0);
    assert_eq!(drawn.panel.height(), 5.0 * 24.0 + 16.0);
    assert_eq!(pending.panel.height(), 27.0 * 24.0 + 9.0 + 16.0);
    assert!(pending.panel.height() <= 800.0, "27 行仍在 1080p 可用区内");

    // 宽度 = 两侧内边距 + 单列行宽（不再有列间距）
    assert_eq!(notes.panel.width(), PADDING * 2.0 + 160.0);
    assert_eq!(pending.panel.width(), PADDING * 2.0 + 160.0);

    // 行数：音符 10（8 常规 + 2 设刀），已 Bake 5，待确认 27（8 常规 + 19 专属）。
    assert_eq!(notes.row_count(), 10);
    assert_eq!(drawn.row_count(), 5);
    assert_eq!(pending.row_count(), 27);
    assert_eq!(notes.groups.len(), 2, "音符：常规组 + 设刀组");
    assert_eq!(pending.groups.len(), 2, "待确认：常规组 + 专属组");
    assert_eq!(drawn.groups.len(), 1, "已 Bake 图形保持单组");
}

#[test]
fn rows_are_contiguous_vertical_stripes() {
    let layout = sample(ContextMenuTarget::PendingDrawnShape);
    let first = item_rect(&layout, 0);
    let second = item_rect(&layout, 1);
    assert_eq!(first.height(), ROW_H);
    assert_eq!(first.bottom(), second.top(), "行与行之间不留缝");

    // 常规组 8 行后是分隔线，再进入专属组。
    let last_regular = item_rect(&layout, 7);
    let first_special = item_rect(&layout, 8);
    assert_eq!(
        first_special.top() - last_regular.bottom(),
        SEPARATOR_H,
        "组间留出分隔线高度"
    );

    // 最后一行不得越出面板内边界。
    let last = item_rect(&layout, 26);
    assert!(last.bottom() <= layout.panel.bottom() - PADDING + 0.01);
}

#[test]
fn out_of_range_item_is_empty() {
    let layout = sample(ContextMenuTarget::DrawnShape);
    assert_eq!(item_rect(&layout, 5), Rect::NOTHING, "越界行");
    assert_eq!(item_rect(&layout, 99), Rect::NOTHING, "越界行");
}

#[test]
fn panel_clamps_into_narrow_available() {
    // 720p：可用区高度 600，27 行内容 673 → 面板钳到 600 并允许滚动。
    let mut m = model(ContextMenuTarget::PendingDrawnShape);
    m.available = Rect::from_min_max(Pos2::new(248.0, 102.0), Pos2::new(1448.0, 702.0));
    m.position = Pos2::new(600.0, 690.0);
    let layout = place(&m, 160.0, &rows(m.target), 0.0);
    assert!(layout.panel.top() >= m.available.top());
    assert!(layout.panel.left() >= m.available.left());
    assert!(layout.panel.bottom() <= m.available.bottom());
    assert!(layout.panel.height() <= 600.0);
    assert!(layout.max_scroll() > 0.0, "放不下时必须可滚动");
}

#[test]
fn scroll_moves_rows_and_clamps() {
    let mut m = model(ContextMenuTarget::PendingDrawnShape);
    m.available = Rect::from_min_max(Pos2::new(248.0, 102.0), Pos2::new(1448.0, 702.0));
    let rows = rows(m.target);

    let top = place(&m, 160.0, &rows, 0.0);
    let scrolled = place(&m, 160.0, &rows, 10.0);
    assert_eq!(scrolled.scroll(), 10.0);
    assert_eq!(
        item_rect(&scrolled, 0).top(),
        item_rect(&top, 0).top() - 10.0
    );

    // 超出上界：钳到最大滚动值，首行滚出视口。
    let clamped = place(&m, 160.0, &rows, 10_000.0);
    assert_eq!(clamped.scroll(), clamped.max_scroll());
    assert!(item_rect(&clamped, 0).bottom() <= clamped.content.top());

    // 负值钳到 0。
    assert_eq!(place(&m, 160.0, &rows, -20.0).scroll(), 0.0);
}

#[test]
fn notes_menu_never_scrolls() {
    let layout = sample(ContextMenuTarget::Notes);
    assert_eq!(layout.max_scroll(), 0.0);
    assert_eq!(layout.content.height(), layout.content_height);
}
