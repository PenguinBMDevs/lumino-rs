//! 调色板缓存刷新回归：同一调色板下 `refresh_palette` 不得改变输出。

use std::sync::Arc;

use lumino_cpu_onion::{CanvasCamera, CpuOnionRenderer, TrackIndex};
use lumino_midi_model::{MidiDocument, NoteEvent};

#[test]
fn refresh_palette_keeps_output_consistent() {
    let mut document = MidiDocument::empty_with_tracks(2, 480);
    document.insert_note(1, NoteEvent::new(10, 110, 120, 100, 0));
    let document = Arc::new(document);
    let index = Arc::new(TrackIndex::build(&document));

    let mut renderer = CpuOnionRenderer::new(2, 2);
    renderer.resize(320, 120);
    let camera = CanvasCamera {
        width: 320,
        height: 120,
        scroll_x: 0.0,
        // key 120 位于画布顶部。
        scroll_y: 140.0,
        zoom_x: 1.0,
        zoom_y: 20.0,
        key_count: 128,
    };

    renderer.render_full(Arc::clone(&document), Arc::clone(&index), &camera, 0, &[]);
    let first = renderer.frame().pixels.clone();
    renderer.refresh_palette(2);
    renderer.render_full(Arc::clone(&document), Arc::clone(&index), &camera, 0, &[]);

    assert_eq!(
        first,
        renderer.frame().pixels,
        "调色板未变时刷新缓存不得改变输出"
    );
    assert_eq!(renderer.visible_notes(), 1, "refresh 后仍应绘制洋葱皮音符");
}
