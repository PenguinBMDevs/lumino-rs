//! 菜单几何单测（DRAW-046 图标 + 文字行列布局）

use super::*;

fn model(target: ContextMenuTarget) -> PianoRollMenuModel {
    PianoRollMenuModel {
        position: Pos2::new(400.0, 300.0),
        available: Rect::from_min_size(Pos2::new(248.0, 102.0), Vec2::new(1200.0, 800.0)),
        target,
    }
}

/// 用固定列宽构造布局，避免单测依赖字体度量。
fn sample(target: ContextMenuTarget) -> MenuLayout {
    let rows: Vec<usize> = menu_columns(target)
        .iter()
        .map(|items| items.len())
        .collect();
    let widths: Vec<f32> = rows.iter().map(|_| 160.0).collect();
    place(&model(target), &widths, &rows)
}

#[test]
fn panel_fits_inside_available_and_flips_up() {
    let rows = [8_usize, 2];
    let widths = [160.0_f32, 150.0];

    // 靠右下角：下方放不下 → 翻到上方，且不越界。
    let mut m = model(ContextMenuTarget::Notes);
    m.position = Pos2::new(1400.0, 880.0);
    let layout = place(&m, &widths, &rows);
    assert!(
        m.available.contains_rect(layout.panel),
        "{:?} 越界",
        layout.panel
    );
    assert!(layout.panel.bottom() <= m.position.y, "应翻到鼠标上方");

    // 靠左上角：正常向下展开。
    let mut m = model(ContextMenuTarget::Notes);
    m.position = Pos2::new(260.0, 110.0);
    let layout = place(&m, &widths, &rows);
    assert!(layout.panel.top() >= m.position.y, "应向下展开");
    assert!(m.available.contains_rect(layout.panel));
}

#[test]
fn panel_size_follows_rows_and_columns() {
    let notes = sample(ContextMenuTarget::Notes);
    let drawn = sample(ContextMenuTarget::DrawnShape);
    let pending = sample(ContextMenuTarget::PendingDrawnShape);

    // 行数预算由消息层声明，几何层照单执行。
    for target in [
        ContextMenuTarget::Notes,
        ContextMenuTarget::DrawnShape,
        ContextMenuTarget::PendingDrawnShape,
    ] {
        for items in menu_columns(target) {
            assert!(items.len() <= lumino_message::context_menu::MAX_ROWS_PER_COLUMN);
        }
    }

    // 高度 = 最大行数 × 24 + 上下内边距 16
    assert_eq!(notes.panel.height(), 8.0 * 24.0 + 16.0);
    assert_eq!(drawn.panel.height(), 5.0 * 24.0 + 16.0);
    assert_eq!(pending.panel.height(), 19.0 * 24.0 + 16.0);
    assert!(pending.panel.height() <= 500.0, "19 行仍在高度上限内");

    // 宽度 = 两侧内边距 + 列宽之和 + 列间距
    assert_eq!(
        notes.panel.width(),
        PADDING * 2.0 + 160.0 * 2.0 + COLUMN_GAP
    );
    assert_eq!(drawn.panel.width(), PADDING * 2.0 + 160.0);
    assert_eq!(notes.column_count(), 2, "音符：常规列 + 设刀列");
    assert_eq!(pending.column_count(), 2, "待确认：常规列 + 专属列");
    assert_eq!(drawn.column_count(), 1, "已 Bake 图形保持单列");
}

#[test]
fn columns_are_left_to_right_with_gap() {
    let layout = sample(ContextMenuTarget::Notes);
    let first = item_rect(&layout, 0, 0);
    let second = item_rect(&layout, 1, 0);
    assert_eq!(first.top(), second.top(), "列间必须顶部对齐");
    assert_eq!(second.left() - first.right(), COLUMN_GAP, "列间距");
    assert_eq!(first.left(), layout.panel.left() + PADDING);
}

#[test]
fn rows_are_contiguous_vertical_stripes() {
    let layout = sample(ContextMenuTarget::PendingDrawnShape);
    let first = item_rect(&layout, 1, 0);
    let second = item_rect(&layout, 1, 1);
    assert_eq!(first.height(), ROW_H);
    assert_eq!(first.bottom(), second.top(), "行与行之间不留缝");
    // 最后一行不得越出面板内边界。
    let last = item_rect(&layout, 1, 18);
    assert!(last.bottom() <= layout.panel.bottom() - PADDING + 0.01);
}

#[test]
fn out_of_range_item_is_empty() {
    let layout = sample(ContextMenuTarget::DrawnShape);
    assert_eq!(item_rect(&layout, 1, 0), Rect::NOTHING, "越界列");
    assert_eq!(item_rect(&layout, 0, 99), Rect::NOTHING, "越界行");
}

#[test]
fn panel_clamps_into_narrow_available() {
    // 720p：可用区高度 600，19 行面板 472 仍装得下。
    let mut m = model(ContextMenuTarget::PendingDrawnShape);
    m.available = Rect::from_min_max(Pos2::new(248.0, 102.0), Pos2::new(1448.0, 702.0));
    m.position = Pos2::new(600.0, 690.0);
    let rows: Vec<usize> = menu_columns(m.target)
        .iter()
        .map(|items| items.len())
        .collect();
    let widths: Vec<f32> = rows.iter().map(|_| 160.0).collect();
    let layout = place(&m, &widths, &rows);
    assert!(layout.panel.top() >= m.available.top());
    assert!(layout.panel.left() >= m.available.left());
    assert!(layout.panel.bottom() <= m.available.bottom());
}
