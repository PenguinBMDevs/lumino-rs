//! 密度缓存生命周期单测：整建 / 增量 / 失效重建。

use lumino_cpu_onion::CanvasCamera;
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
    let pyramid = cache
        .ensure(doc_gen, document, 128, 1)
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
fn apply_changes_updates_cells_without_rebuild() {
    let document = document();
    let mut cache = DensityCache::default();
    assert_eq!(alpha(&mut cache, 1, &document), 255);

    // 删除：上一代缓存 → 减 1。
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
    assert_eq!(alpha(&mut cache, 2, &document), 0, "删除后密度格必须清空");

    // 插入：再 +1 恢复。
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
fn out_of_layout_change_drops_cache_for_rebuild() {
    let document = document();
    let mut cache = DensityCache::default();
    assert_eq!(alpha(&mut cache, 1, &document), 255);

    // 越过既有 bin 布局：必须拒绝增量并丢弃缓存，等下一次 ensure 整建。
    cache.apply_changes(
        Some(1),
        2,
        &[CpuOnionChange {
            track: 1,
            old: None,
            new: Some(NoteEvent::new(10_000_000, 10_001_000, 60, 100, 0)),
        }],
    );
    assert!(
        cache.pyramid.is_none(),
        "非法增量必须丢弃缓存，绝不把音符塞进错误桶"
    );
    assert_eq!(alpha(&mut cache, 2, &document), 255, "整建后必须恢复");
}
