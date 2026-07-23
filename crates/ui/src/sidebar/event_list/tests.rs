//! 事件列表视图测试

use lumino_core::im::Vector;
use lumino_core::note::Note;

use super::{EventListCanvas, HEADER_HEIGHT, ROW_HEIGHT};

fn sample_notes() -> Vector<Note> {
    let mut notes = Vector::new();
    notes.push_back(Note::new(0.0, 60, 480.0).with_velocity(100));
    notes.push_back(Note::new(480.0, 62, 480.0).with_velocity(90));
    notes.push_back(Note::new(960.0, 64, 240.0).with_velocity(80));
    notes
}

#[test]
fn test_total_height() {
    let notes = sample_notes();
    let canvas = EventListCanvas::new(&notes, 480, 120.0, 0.0, 100.0);
    assert_eq!(canvas.total_height(), HEADER_HEIGHT + 3.0 * ROW_HEIGHT);
}

#[test]
fn test_visible_range_empty() {
    let empty = Vector::new();
    let canvas = EventListCanvas::new(&empty, 480, 120.0, 0.0, 100.0);
    assert_eq!(canvas.visible_range(0.0, 100.0), (0, 0));
}

#[test]
fn test_visible_range_clamped() {
    let notes = sample_notes();
    let canvas = EventListCanvas::new(&notes, 480, 120.0, 0.0, 36.0);
    // 视口高度 36px：扣除 20px 表头后剩余 16px，约 1 行可见，含过度绘制共 2 行
    let (first, last) = canvas.visible_range(0.0, 36.0);
    assert_eq!(first, 0);
    assert_eq!(last, 2);
}

#[test]
fn test_visible_range_scrolled() {
    let notes = sample_notes();
    let canvas = EventListCanvas::new(&notes, 480, 120.0, ROW_HEIGHT, ROW_HEIGHT * 2.0);
    // 滚动超过表高 + 一行后，first 至少为 1
    let scroll_y = HEADER_HEIGHT + ROW_HEIGHT;
    let (first, last) = canvas.visible_range(scroll_y, ROW_HEIGHT * 2.0);
    assert!(first >= 1);
    assert!(last <= notes.len());
}
