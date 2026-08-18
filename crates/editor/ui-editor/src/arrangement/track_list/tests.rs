//! 工程走带左侧音轨列表 —— 交互测试
//!
//! 从 `track_list.rs` 抽出，控制文件行数并保持单一职责。

use iced_core::mouse::Cursor;
use iced_core::{Point, Rectangle, Size};
use iced_widget::canvas::{self, Program};

use super::*;
use crate::Message;

fn bounds() -> Rectangle {
    Rectangle::new(Point::new(0.0, 0.0), Size::new(160.0, 600.0))
}

fn canvas(ctrl: bool, zoom_y: f32) -> TrackListCanvas {
    TrackListCanvas::new(vec![(0, "A".into()), (1, "B".into())], 0, 0.0, 48.0, 96.0)
        .with_zoom_y(zoom_y)
        .with_ctrl_pressed(ctrl)
}

fn wheel(delta: iced_core::mouse::ScrollDelta) -> canvas::Event {
    canvas::Event::Mouse(iced_core::mouse::Event::WheelScrolled { delta })
}

/// Ctrl+滚轮：垂直缩放，倍率按卷帘式平滑步进（每刻度 ±10%），
/// 锚点比例为鼠标在列表内的纵向相对位置。
#[test]
fn test_ctrl_wheel_zooms_y_around_pointer() {
    let canvas = canvas(true, 1.0);
    let mut state = TrackListState::default();
    let cursor = Cursor::Available(Point::new(80.0, 300.0));
    let action = canvas
        .update(
            &mut state,
            &wheel(iced_core::mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 }),
            bounds(),
            cursor,
        )
        .expect("Ctrl+滚轮应产生垂直缩放动作");
    let (message, _, _) = action.into_inner();
    match message {
        Some(Message::ArrangementZoomY { zoom, fixed_ratio }) => {
            // zoom_y(1.0) * 因子(1 + 1*0.1) = 1.1
            assert!((zoom - 1.1).abs() < f32::EPSILON, "zoom = {zoom}");
            // 鼠标位于列表纵向中点（300/600）
            assert!(
                (fixed_ratio - 0.5).abs() < f32::EPSILON,
                "fixed_ratio = {fixed_ratio}"
            );
        }
        other => panic!("Ctrl+滚轮音轨列表应发 ArrangementZoomY，实际为: {other:?}"),
    }
}

/// Ctrl+滚轮向下滚动（y < 0）→ 缩小
#[test]
fn test_ctrl_wheel_zooms_y_out() {
    let canvas = canvas(true, 2.0);
    let mut state = TrackListState::default();
    let cursor = Cursor::Available(Point::new(80.0, 150.0));
    let action = canvas
        .update(
            &mut state,
            &wheel(iced_core::mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 }),
            bounds(),
            cursor,
        )
        .expect("Ctrl+滚轮应产生垂直缩放动作");
    let (message, _, _) = action.into_inner();
    match message {
        Some(Message::ArrangementZoomY { zoom, fixed_ratio }) => {
            // zoom_y(2.0) * 因子(1 - 1*0.1) = 1.8
            assert!((zoom - 1.8).abs() < f32::EPSILON, "zoom = {zoom}");
            assert!(
                (fixed_ratio - 0.25).abs() < f32::EPSILON,
                "fixed_ratio = {fixed_ratio}"
            );
        }
        other => panic!("Ctrl+滚轮音轨列表应发 ArrangementZoomY，实际为: {other:?}"),
    }
}

/// Ctrl+滚轮但增量为 0 → 无操作（避免旧式 dy<=0 误判缩小的缺陷）
#[test]
fn test_ctrl_wheel_zero_delta_is_noop() {
    let canvas = canvas(true, 1.0);
    let mut state = TrackListState::default();
    let cursor = Cursor::Available(Point::new(80.0, 300.0));
    assert!(
        canvas
            .update(
                &mut state,
                &wheel(iced_core::mouse::ScrollDelta::Lines { x: 1.0, y: 0.0 }),
                bounds(),
                cursor
            )
            .is_none()
    );
}

/// 未按 Ctrl：普通滚轮仍为垂直滚动（既有行为不变）
#[test]
fn test_plain_wheel_still_scrolls_y() {
    let canvas = canvas(false, 1.0);
    let mut state = TrackListState::default();
    let cursor = Cursor::Available(Point::new(80.0, 300.0));
    let action = canvas
        .update(
            &mut state,
            &wheel(iced_core::mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 }),
            bounds(),
            cursor,
        )
        .expect("普通滚轮应产生滚动动作");
    let (message, _, _) = action.into_inner();
    match message {
        Some(Message::ArrangementScrollY(y)) => {
            // scroll_y(0.0) - dy(1 * SCROLL_LINES_SCALE = 30) = -30（由 Root 钳制）
            assert!((y - -30.0).abs() < f32::EPSILON, "y = {y}");
        }
        other => panic!("普通滚轮音轨列表应发 ArrangementScrollY，实际为: {other:?}"),
    }
}

/// BUG 回归：鼠标在右侧音符区（本 Canvas bounds 之外）时，
/// Ctrl+滚轮不得触发本列表的 Y 向缩放——
/// iced 0.14 事件全树分发，无位置检查会与音符区 X 向缩放双触发。
#[test]
fn test_ctrl_wheel_outside_bounds_does_not_zoom_y() {
    let canvas = canvas(true, 1.0);
    let mut state = TrackListState::default();
    // 鼠标在音符区（x = 400 超出列表宽度 160）
    let cursor = Cursor::Available(Point::new(400.0, 300.0));
    assert!(
        canvas
            .update(
                &mut state,
                &wheel(iced_core::mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 }),
                bounds(),
                cursor,
            )
            .is_none(),
        "鼠标在列表外时 Ctrl+滚轮不应产生任何缩放动作"
    );
}

/// BUG 回归（同类根因）：鼠标在音符区普通滚轮时，本列表不得
/// 再发 ArrangementScrollY（否则与音符区滚动双触发、滚动量翻倍）。
#[test]
fn test_plain_wheel_outside_bounds_does_not_scroll() {
    let canvas = canvas(false, 1.0);
    let mut state = TrackListState::default();
    let cursor = Cursor::Available(Point::new(400.0, 300.0));
    assert!(
        canvas
            .update(
                &mut state,
                &wheel(iced_core::mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 }),
                bounds(),
                cursor,
            )
            .is_none(),
        "鼠标在列表外时普通滚轮不应产生滚动动作"
    );
}

/// BUG 回归（同类根因）：鼠标在音符区按下左键时，本列表不得
/// 误选音轨/注册拖拽排序（iced 全树分发 + 无位置检查的既有缺陷）。
#[test]
fn test_left_press_outside_bounds_is_noop() {
    let canvas = canvas(false, 1.0);
    let mut state = TrackListState::default();
    let cursor = Cursor::Available(Point::new(400.0, 300.0));
    assert!(
        canvas
            .update(
                &mut state,
                &canvas::Event::Mouse(iced_core::mouse::Event::ButtonPressed(
                    iced_core::mouse::Button::Left,
                )),
                bounds(),
                cursor,
            )
            .is_none(),
        "鼠标在列表外按下左键不应产生选择/拖拽动作"
    );
    assert!(state.drag.is_none(), "列表外按下不应注册拖拽候选");
}

/// 列表内按下左键仍正常选中（既有行为回归保护）
#[test]
fn test_left_press_inside_bounds_selects_track() {
    let canvas = canvas(false, 1.0);
    let mut state = TrackListState::default();
    let cursor = Cursor::Available(Point::new(80.0, 60.0));
    let action = canvas
        .update(
            &mut state,
            &canvas::Event::Mouse(iced_core::mouse::Event::ButtonPressed(
                iced_core::mouse::Button::Left,
            )),
            bounds(),
            cursor,
        )
        .expect("列表内按下左键应产生选择动作");
    let (message, _, _) = action.into_inner();
    match message {
        Some(Message::Batch(messages)) => {
            // 按下发布的是 Batch[TrackSelected, TracksSelected, TrackReorderStarted]
            assert!(
                messages.iter().any(|m| matches!(
                    m,
                    Message::Sidebar(lumino_ui_core::sidebar_event::Event::TrackSelected(..))
                )),
                "Batch 应包含 TrackSelected，实际为: {messages:?}"
            );
        }
        other => panic!("列表内按下应发 Batch(TrackSelected)，实际为: {other:?}"),
    }
}
