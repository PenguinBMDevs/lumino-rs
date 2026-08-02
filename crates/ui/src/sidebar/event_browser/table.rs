//! 事件浏览器表格纯逻辑。
//!
//! 本模块只包含分页、行选择、`SegmentShape` 文本化等与 UI 框架无关的逻辑。
//! 任何 egui 或 iced Canvas 的绘制代码均不放在此处。

use lumino_extras::i18n::MainTranslations;
use lumino_note_core::event::SegmentShape;

use super::state::EventBrowserState;

/// 每页行数。100 行在常规字体下约填满半屏~一屏，翻页频率适中。
pub(super) const EVENT_PAGE_SIZE: usize = 100;

/// 计算总页数（至少 1 页）。
pub(super) fn total_pages(total: usize) -> usize {
    total.div_ceil(EVENT_PAGE_SIZE).max(1)
}

/// 根据当前 `state.event_page` 切片出当前页。
///
/// 返回 `(page, page_start, page_slice)`：
/// - `page`：0-based 页码（已做越界保护，删除数据后自动夹回末页）
/// - `page_start`：当前页起始索引
/// - `page_slice`：当前页的切片
#[allow(dead_code)] // 由测试覆盖；Canvas 侧使用不可变 page_slice 变体
pub(super) fn paginate<'a, T>(
    state: &mut EventBrowserState,
    items: &'a [T],
) -> (usize, usize, &'a [T]) {
    let total = items.len();
    let tp = total_pages(total);
    if state.event_page >= tp {
        state.event_page = tp - 1;
    }
    let page = state.event_page;
    let start = page * EVENT_PAGE_SIZE;
    let end = (start + EVENT_PAGE_SIZE).min(total);
    (page, start, &items[start..end])
}

/// 处理行点击的多选逻辑（Ctrl 切换、Shift 范围、普通单选）。
///
/// - 仅 `ctrl`：切换当前 tick 的选中状态
/// - 仅 `shift`：从 `last_clicked_tick` 到当前 tick 的范围选择
/// - 无修饰键：清空后单选当前 tick，并设置 `last_clicked_tick`
///
/// `all_ticks` 用于 Shift 范围选择时按实际存在的事件 tick 过滤。
#[allow(dead_code)] // 由测试覆盖；多选交互经 Sidebar 消息路径处理
pub(super) fn handle_row_click(
    state: &mut EventBrowserState,
    tick: u32,
    all_ticks: &[u32],
    ctrl: bool,
    shift: bool,
) {
    if ctrl {
        // Ctrl：切换该行选中
        if state.selected_ticks.contains(&tick) {
            state.selected_ticks.remove(&tick);
        } else {
            state.selected_ticks.insert(tick);
        }
        state.last_clicked_tick = Some(tick);
    } else if shift {
        // Shift：范围选择（从上次点击到当前）
        if let Some(anchor) = state.last_clicked_tick {
            let (lo, hi) = if anchor <= tick {
                (anchor, tick)
            } else {
                (tick, anchor)
            };
            for &t in all_ticks {
                if t >= lo && t <= hi {
                    state.selected_ticks.insert(t);
                }
            }
        } else {
            state.selected_ticks.clear();
            state.selected_ticks.insert(tick);
        }
    } else {
        // 普通单击：只选该行
        state.selected_ticks.clear();
        state.selected_ticks.insert(tick);
        state.last_clicked_tick = Some(tick);
    }
}

/// 把 `SegmentShape` 格式化为表格单元格文本（仅类型名）。
pub(super) fn shape_text(shape: SegmentShape, t: &MainTranslations) -> String {
    match shape {
        SegmentShape::Step => t.eb_step.to_string(),
        SegmentShape::Curve { .. } => t.eb_curve.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn total_pages_zero_or_empty() {
        assert_eq!(total_pages(0), 1);
        assert_eq!(total_pages(1), 1);
        assert_eq!(total_pages(EVENT_PAGE_SIZE), 1);
        assert_eq!(total_pages(EVENT_PAGE_SIZE + 1), 2);
        assert_eq!(total_pages(EVENT_PAGE_SIZE * 3), 3);
    }

    #[test]
    fn paginate_first_page() {
        let mut state = EventBrowserState::default();
        let items: Vec<u32> = (0..250).collect();
        let (page, start, slice) = paginate(&mut state, &items);
        assert_eq!(page, 0);
        assert_eq!(start, 0);
        assert_eq!(slice.len(), EVENT_PAGE_SIZE);
        assert_eq!(slice[0], 0);
        assert_eq!(slice[EVENT_PAGE_SIZE - 1], EVENT_PAGE_SIZE as u32 - 1);
    }

    #[test]
    fn paginate_clamps_out_of_bounds_page() {
        let mut state = EventBrowserState::default();
        state.event_page = 10;
        let items: Vec<u32> = (0..50).collect();
        let (page, start, slice) = paginate(&mut state, &items);
        assert_eq!(page, 0);
        assert_eq!(start, 0);
        assert_eq!(slice.len(), 50);
    }

    #[test]
    fn handle_row_click_single_select_clears_previous() {
        let mut state = EventBrowserState::default();
        state.selected_ticks.insert(10);
        state.selected_ticks.insert(20);

        handle_row_click(&mut state, 30, &[10, 20, 30, 40], false, false);

        assert_eq!(state.selected_ticks.len(), 1);
        assert!(state.selected_ticks.contains(&30));
        assert_eq!(state.last_clicked_tick, Some(30));
    }

    #[test]
    fn handle_row_click_ctrl_toggles() {
        let mut state = EventBrowserState::default();
        handle_row_click(&mut state, 10, &[10, 20, 30], true, false);
        assert!(state.selected_ticks.contains(&10));

        handle_row_click(&mut state, 20, &[10, 20, 30], true, false);
        assert!(state.selected_ticks.contains(&10));
        assert!(state.selected_ticks.contains(&20));

        handle_row_click(&mut state, 10, &[10, 20, 30], true, false);
        assert!(!state.selected_ticks.contains(&10));
        assert!(state.selected_ticks.contains(&20));
    }

    #[test]
    fn handle_row_click_shift_range_select() {
        let mut state = EventBrowserState::default();
        state.last_clicked_tick = Some(20);
        let all_ticks = vec![10, 15, 20, 25, 30, 35];

        handle_row_click(&mut state, 30, &all_ticks, false, true);

        assert!(state.selected_ticks.contains(&20));
        assert!(state.selected_ticks.contains(&25));
        assert!(state.selected_ticks.contains(&30));
        assert!(!state.selected_ticks.contains(&10));
        assert!(!state.selected_ticks.contains(&35));
    }

    #[test]
    fn handle_row_click_shift_without_anchor_falls_back_to_single() {
        let mut state = EventBrowserState::default();
        let all_ticks = vec![10, 20, 30];

        handle_row_click(&mut state, 20, &all_ticks, false, true);

        assert_eq!(state.selected_ticks.len(), 1);
        assert!(state.selected_ticks.contains(&20));
    }

    #[test]
    fn handle_row_click_ctrl_sets_last_clicked() {
        let mut state = EventBrowserState::default();
        handle_row_click(&mut state, 42, &[42], true, false);
        assert_eq!(state.last_clicked_tick, Some(42));
    }
}
