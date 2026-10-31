//! 密度缓存生命周期单测：整建 / 增量 / 失效重建。

use lumino_cpu_onion::{CanvasCamera, TrackIndex};
use lumino_editor_state::CpuOnionChange;
use lumino_midi_model::{MidiDocument, NoteEvent};

use super::DensityCache;

const WIDTH: u32 = 64;
const HEIGHT: u32 = 64;
const KEY: u16 = 125;

fn camera() -> CanvasCamera {
    CanvasCamera {
        width: WIDTH,
        height: HEIGHT,
        scroll_x: 0.0,
        scroll_y: 0.0,
        zoom_x: 0.01,
        zoom_y: 20.0,
        key_count: 128,
    }
}

fn note() -> NoteEvent {
    NoteEvent::new(0, 2000, KEY as u8, 100, 0)
}

fn document() -> MidiDocument {
    let mut document = MidiDocument::empty_with_tracks(2, 480);
    document.insert_note(1, note());
    document
}

/// 渲染一次总览，返回 key 125 首列的 alpha（255 = 有密度，0 = 透明）。
fn alpha(cache: &mut DensityCache, doc_gen: u64, document: &MidiDocument) -> u8 {
    let index = TrackIndex::build(document);
    let pyramid = cache
        .ensure(doc_gen, document, &index, 128, 1)
        .expect("密度金字塔必须可用");
    let camera = camera();
    let mut frame = vec![0u32; (WIDTH * HEIGHT) as usize];
    pyramid.render_overview(&mut frame, WIDTH, HEIGHT, &camera);
    let row = (camera.key_to_y(KEY) + camera.zoom_y * 0.5) as u32;
    ((frame[(row * WIDTH) as usize] >> 24) & 0xFF) as u8
}

#[test]
fn ensure_builds_then_keeps_cache_per_generation() {
    let document = document();
    let mut cache = DensityCache::default();
    assert_eq!(alpha(&mut cache, 1, &document), 255);
    assert_eq!(cache.built_gen, Some(1));
    // 同代次复用，不重建（built_gen 不变）。
    assert_eq!(alpha(&mut cache, 1, &document), 255);
    assert_eq!(cache.built_gen, Some(1));
    // 代次变化 → 重建。
    assert_eq!(alpha(&mut cache, 2, &document), 255);
    assert_eq!(cache.built_gen, Some(2));
}

#[test]
fn apply_changes_updates_cells_and_rebuilds_stale_coverage() {
    let mut document = document();
    let mut cache = DensityCache::default();
    assert_eq!(alpha(&mut cache, 1, &document), 255);

    // 删除：计数增量已生效，覆盖位图标记脏区；下一次 ensure 只重建脏区间。
    assert!(document.remove_note(1, 0).is_some());
    cache.apply_changes(
        Some(1),
        2,
        &[CpuOnionChange {
            track: 1,
            old: Some(note()),
            new: None,
        }],
    );
    assert_eq!(cache.built_gen, Some(2), "增量成功必须推进代次");
    assert!(
        !cache
            .pyramid
            .as_ref()
            .is_some_and(|pyramid| pyramid.coverage_valid()),
        "覆盖位图必须标记失效"
    );
    assert_eq!(alpha(&mut cache, 2, &document), 0, "删除后密度格必须清空");
    assert!(
        cache
            .pyramid
            .as_ref()
            .is_some_and(|pyramid| pyramid.coverage_valid()),
        "脏区局部重建后覆盖层必须恢复可用"
    );

    // 插入：再 +1 恢复。
    assert!(document.insert_note(1, note()));
    cache.apply_changes(
        Some(2),
        3,
        &[CpuOnionChange {
            track: 1,
            old: None,
            new: Some(note()),
        }],
    );
    assert_eq!(alpha(&mut cache, 3, &document), 255);
}

#[test]
fn out_of_layout_change_grows_cache_without_rebuild() {
    let mut edited = document();
    let note = NoteEvent::new(3000, 3200, 60, 100, 0);
    edited.insert_note(1, note);
    let mut cache = DensityCache::default();
    // 先用旧文档建缓存，再模拟插入越界音符。
    let old_document = document();
    assert_eq!(alpha(&mut cache, 1, &old_document), 255);

    cache.apply_changes(
        Some(1),
        2,
        &[CpuOnionChange {
            track: 1,
            old: None,
            new: Some(note),
        }],
    );
    assert_eq!(cache.built_gen, Some(2), "越界变更应增量接收并扩容");
    let index = TrackIndex::build(&edited);
    assert!(
        cache
            .pyramid
            .as_mut()
            .is_some_and(|pyramid| pyramid.rebuild_dirty_coverage(&edited, &index)),
        "扩容后脏区局部重建必须成功"
    );
    assert!(
        cache
            .pyramid
            .as_ref()
            .is_some_and(|pyramid| pyramid.total_ticks() >= 3200),
        "渲染右界必须扩到新音符末尾"
    );
}
