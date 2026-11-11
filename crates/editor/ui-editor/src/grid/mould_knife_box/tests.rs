//! 饼干模「刀」可视化测试

use super::*;
use crate::Editor;
use lumino_editor_state::ShapeNote;

fn knife_note(key: u16, tick: f32, length: f32) -> ShapeNote {
    ShapeNote {
        track: 1,
        tick,
        key,
        length,
        velocity: 100,
        channel: 0,
    }
}

#[test]
fn test_knife_rects_empty_without_knife() {
    let editor = Editor::new();
    assert!(knife_rects(&editor).is_empty(), "没有刀时不应画虚线框");
}

#[test]
fn test_knife_rects_one_per_note() {
    let mut editor = Editor::new();
    editor.mould_walls = Some(vec![
        knife_note(60, 0.0, 100.0),
        knife_note(60, 200.0, 100.0),
    ]);
    let rects = knife_rects(&editor);
    assert_eq!(rects.len(), 2, "每条刀音符一个虚线框");
    assert!(rects.iter().all(|r| r.width > 0.0 && r.height > 0.0));
}

#[test]
fn test_knife_rect_matches_note_row() {
    // 回归：框顶边必须等于 key_to_y(key)，不能用 key+1（会整体上移一行）
    let mut editor = Editor::new();
    editor.mould_walls = Some(vec![knife_note(60, 0.0, 100.0), knife_note(64, 0.0, 100.0)]);
    let rects = knife_rects(&editor);
    let view = &editor.editor_state.view;
    for (rect, key) in rects.iter().zip([60_u16, 64]) {
        let expected = view.key_to_y(key);
        assert!(
            (rect.y - expected).abs() < 1e-3,
            "key {key} 的框顶边应为 {expected}，实际 {}",
            rect.y
        );
        assert!(
            (rect.height - view.zoom_y).abs() < 1e-3,
            "框高应等于一个 key 行"
        );
    }
}

#[test]
fn test_knife_rect_equals_note_screen_bounds() {
    // 对齐的唯一权威：音符命中/绘制用的 `NoteExt::screen_bounds`
    use crate::note::NoteExt;

    let mut editor = Editor::new();
    editor.mould_walls = Some(vec![knife_note(60, 100.0, 200.0)]);
    let rect = knife_rects(&editor)[0];
    let expected = crate::Note::new(100.0, 60, 200.0).screen_bounds(&editor.editor_state.view);
    assert!((rect.x - expected.x).abs() < 1e-3, "x 应与音符一致");
    assert!((rect.y - expected.y).abs() < 1e-3, "y 应与音符一致");
    assert!(
        (rect.width - expected.width).abs() < 1e-3,
        "宽度应与音符一致"
    );
    assert!(
        (rect.height - expected.height).abs() < 1e-3,
        "高度应与音符一致"
    );
}
