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
fn clearing_track_uses_dirty_range_not_full_build() {
    let mut document = document();
    let mut cache = DensityCache::default();
    assert_eq!(alpha(&mut cache, 1, &document), 255);
    assert_eq!(cache.full_builds, 1);

    // 物理删轨：整轨音符清空，只提交该轨覆盖的 tick 区间。
    assert!(document.replace_track_notes(1, Vec::new()));
    cache.apply_ranges(Some(1), 2, &[(0, 2000)]);
    assert_eq!(alpha(&mut cache, 2, &document), 0);
    assert_eq!(cache.range_rebuilds, 1, "减轨必须走区间重建");
    assert_eq!(cache.full_builds, 1, "减轨不得整建");
}

#[test]
fn switching_to_256_keys_grows_without_full_build() {
    let mut document = document();
    let index = TrackIndex::build(&document);
    let mut cache = DensityCache::default();
    cache
        .ensure(1, &document, &index, 128, 1)
        .expect("首次密度必须构建");
    assert_eq!(cache.full_builds, 1);

    // 文档还没有高 key：切 256 只复用 128 金字塔，不整建。
    let pyramid = cache
        .ensure(1, &document, &index, 256, 1)
        .expect("切 256 必须复用");

    // 128 金字塔在 256 视口下高 key 必须透明，不能越界读下一 bin。
    let high_camera = CanvasCamera {
        key_count: 256,
        scroll_y: (255 - 200) as f32 * 20.0,
        ..camera()
    };
    let mut empty = vec![0u32; (WIDTH * HEIGHT) as usize];
    pyramid.render_overview(&mut empty, WIDTH, HEIGHT, &high_camera);
    let row = (high_camera.key_to_y(200) + high_camera.zoom_y * 0.5) as u32;
    assert_eq!(
        ((empty[(row * WIDTH) as usize] >> 24) & 0xFF) as u8,
        0,
        "未覆盖的高 key 必须是透明"
    );
    assert_eq!(cache.full_builds, 1);
    assert_eq!(cache.built_keys, 128);

    // 插入 key 200 后切 256：只扩 key 列 + 扫该轨，不整建。
    document.insert_note(1, NoteEvent::new(0, 2000, 200, 100, 0));
    let index = TrackIndex::build(&document);
    let pyramid = cache
        .ensure(1, &document, &index, 256, 1)
        .expect("扩 key 后必须可用");
    assert_eq!(pyramid.key_count(), 256);

    let camera = CanvasCamera {
        key_count: 256,
        scroll_y: (255 - 200) as f32 * 20.0,
        ..camera()
    };
    let mut frame = vec![0u32; (WIDTH * HEIGHT) as usize];
    pyramid.render_overview(&mut frame, WIDTH, HEIGHT, &camera);
    let row = (camera.key_to_y(200) + camera.zoom_y * 0.5) as u32;
    assert_eq!(
        ((frame[(row * WIDTH) as usize] >> 24) & 0xFF) as u8,
        255,
        "高 key 必须被扩进密度"
    );
    assert_eq!(cache.full_builds, 1, "256-key 切换不得整建");

    // 切回 128 复用 256 金字塔，不整建。
    cache
        .ensure(1, &document, &index, 128, 1)
        .expect("切回 128 必须复用");
    assert_eq!(cache.full_builds, 1);
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
fn dirty_ranges_rebuild_without_full_build() {
    let mut document = document();
    let mut cache = DensityCache::default();
    assert_eq!(alpha(&mut cache, 1, &document), 255);
    assert!(document.remove_note(1, 0).is_some());

    cache.apply_ranges(Some(1), 2, &[(0, 2000)]);
    assert_eq!(cache.range_rebuilds, 0, "apply 只登记区间");
    assert_eq!(alpha(&mut cache, 2, &document), 0);
    assert_eq!(cache.range_rebuilds, 1, "必须走区间重建，不得整建");
    assert_eq!(cache.built_gen, Some(2));
}

#[test]
fn extreme_out_of_layout_ranges_coarsen_without_full_build() {
    let mut edited = document();
    let tick = 2_200_000u32;
    let note = NoteEvent::new(tick, tick + 100, 60, 100, 0);
    edited.insert_note(1, note);
    let mut cache = DensityCache::default();
    assert_eq!(alpha(&mut cache, 1, &document()), 255);

    cache.apply_ranges(Some(1), 2, &[(tick, tick + 100)]);
    assert_eq!(alpha(&mut cache, 2, &edited), 255);
    assert_eq!(cache.range_rebuilds, 1, "极端越界必须降采样，不得整建");
    assert!(
        cache
            .pyramid
            .as_ref()
            .is_some_and(|pyramid| pyramid.total_ticks() >= tick + 100)
    );
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
