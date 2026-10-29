//! `draw_track` 同 `start_tick` 长/短音符绘制顺序（GPU 源顺序）与边界回归。
//!
//! 判据是**文档源顺序**（`ChunkedList::iter_window` 的全局索引升序），不是
//! 「长音优先 / 短音优先」。旧实现 `take_long = long.start_tick <= window.start_tick`
//! 在 tie 时固定长音先画，短音先插入时会与 GPU 不一致——本文件的 tie 用例在
//! 旧实现下必然失败（见 `same_start_short_first_draws_short_before_long`）。

use std::sync::Arc;

use lumino_midi_model::{MidiDocument, NoteEvent};

use super::{BandJob, BandStats, draw_tick_group, render_band};
use crate::{
    buffer::BandBuffer, camera::CanvasCamera, color::NoteStyle, index::TrackIndex, simd::SimdLevel,
};

/// band 宽度（1px/tick，start < 1200 的音符可见；长音可跨出右界被裁剪）。
const BAND_WIDTH: u32 = 1200;
/// band 高度：zoom_y = 2 时覆盖 128 键。
const BAND_HEIGHT: u32 = 256;
/// 纵向缩放（px/key）。
const ZOOM_Y: f32 = 2.0;
/// 键数。
const KEY_COUNT: u16 = 128;

/// 用给定插入顺序构造 1 轨文档（同 tick 的插入顺序 = 源索引顺序）。
fn document(notes: &[NoteEvent]) -> MidiDocument {
    let mut document = MidiDocument::empty_with_tracks(1, 480);
    for note in notes {
        assert!(document.insert_note(0, *note), "音符必须插入成功");
    }
    document
}

/// 构造 band 作业：`current_track = 1`（文档只有 0 轨）表示不跳过任何轨。
fn job_for(
    document: Arc<MidiDocument>,
    tick_lo: u32,
    tick_hi: u32,
    key_min: u16,
    key_max: u16,
) -> BandJob {
    let index = Arc::new(TrackIndex::build(document.as_ref()));
    BandJob {
        x0: 0,
        x1: BAND_WIDTH,
        camera: CanvasCamera {
            width: BAND_WIDTH,
            height: BAND_HEIGHT,
            scroll_x: 0.0,
            scroll_y: 0.0,
            zoom_x: 1.0,
            zoom_y: ZOOM_Y,
            key_count: KEY_COUNT,
        },
        tick_lo,
        tick_hi,
        key_min,
        key_max,
        current_track: 1,
        muted: Arc::from(Vec::<bool>::new()),
        style: Arc::new(NoteStyle::new(1)),
        index,
        document,
        simd: SimdLevel::detect(),
    }
}

/// 渲染一条 band，返回统计（测试构建里带 `draw_order`）。
fn render(
    document: &MidiDocument,
    tick_lo: u32,
    tick_hi: u32,
    key_min: u16,
    key_max: u16,
) -> BandStats {
    let job = job_for(
        Arc::new(document.clone()),
        tick_lo,
        tick_hi,
        key_min,
        key_max,
    );
    let mut buffer = BandBuffer::default();
    render_band(&job, &mut buffer)
}

/// 同 start_tick：短音先插入（源索引更小）→ 必须先画短音，与 GPU 源顺序一致。
///
/// 旧实现 tie 时 `take_long = true` 固定长音先画，本用例会拿到 `[长, 短]` 而失败。
#[test]
fn same_start_short_first_draws_short_before_long() {
    let short = NoteEvent::new(100, 300, 60, 100, 0); // 200 tick ≤ LOOKBACK
    let long = NoteEvent::new(100, 1100, 62, 100, 0); // 1000 tick > LOOKBACK
    let stats = render(&document(&[short, long]), 0, 1200, 60, 70);
    assert_eq!(
        stats.draw_order,
        vec![short, long],
        "短音先插入时必须短→长（源顺序），不得按「长音先」固定顺序"
    );
    assert_eq!(stats.visible_notes, 2, "两条音符都必须真正画出");
}

/// 同 start_tick：长音先插入 → 先画长音（源顺序，不是「长音永远优先」）。
#[test]
fn same_start_long_first_draws_long_before_short() {
    let long = NoteEvent::new(100, 1100, 62, 100, 0);
    let short = NoteEvent::new(100, 300, 60, 100, 0);
    let stats = render(&document(&[long, short]), 0, 1200, 60, 70);
    assert_eq!(
        stats.draw_order,
        vec![long, short],
        "长音先插入时必须长→短（仍按源顺序）"
    );
    assert_eq!(stats.visible_notes, 2);
}

/// 同 start 多条长音 + 多条短音混插：顺序必须完全等于文档源顺序。
#[test]
fn same_start_mixed_notes_follow_document_order() {
    let a = NoteEvent::new(100, 300, 60, 100, 0); // 短
    let b = NoteEvent::new(100, 1100, 62, 100, 0); // 长
    let c = NoteEvent::new(100, 400, 64, 100, 0); // 短
    let d = NoteEvent::new(100, 1600, 66, 100, 0); // 长
    let stats = render(&document(&[a, b, c, d]), 0, 1200, 60, 70);
    assert_eq!(
        stats.draw_order,
        vec![a, b, c, d],
        "同 tick 混插必须严格等于文档 iter() 源顺序"
    );
    assert_eq!(stats.visible_notes, 4);
}

/// 不同 start_tick 的长/短音符仍按 start 归并（tie 组查询不得打乱常路径）。
#[test]
fn different_starts_still_merge_by_start_tick() {
    let n100 = NoteEvent::new(100, 200, 60, 100, 0); // 短
    let n200 = NoteEvent::new(200, 1200, 62, 100, 0); // 长
    let n300 = NoteEvent::new(300, 400, 64, 100, 0); // 短
    let n400 = NoteEvent::new(400, 1300, 66, 100, 0); // 长
    let n500 = NoteEvent::new(500, 600, 68, 100, 0); // 短
    // 乱序插入：容器内部仍按 start_tick 排序。
    let stats = render(&document(&[n300, n500, n100, n400, n200]), 0, 1400, 60, 70);
    assert_eq!(
        stats.draw_order,
        vec![n100, n200, n300, n400, n500],
        "长短混合流必须按 start_tick 归并"
    );
    assert_eq!(stats.visible_notes, 5);
}

/// 组内 key 越界 / `end < tick_lo` 不画，但两条流推进正确（后续音符不漏）。
#[test]
fn tick_group_filters_bad_notes_without_stalling_streams() {
    // end < tick_lo：长音在 long_notes 里、短音在窗口流里，都必须被丢弃。
    let long_ends_before_window = NoteEvent::new(0, 550, 62, 100, 0); // 550 > 512 且 end < 600
    let short_ends_before_window = NoteEvent::new(100, 500, 60, 100, 0); // end < 600
    // 同 tick 组内 key 越界（key_min = 60）：参与组查询但不得绘制。
    let short_key_out = NoteEvent::new(100, 612, 20, 100, 0); // 512 tick，边界上仍算窗口音符
    let long_in_group = NoteEvent::new(100, 1100, 62, 100, 0);
    let short_in_group = NoteEvent::new(100, 612, 64, 100, 0);
    let short_later = NoteEvent::new(700, 712, 66, 100, 0);
    let long_later = NoteEvent::new(700, 1700, 68, 100, 0);
    let short_last = NoteEvent::new(800, 812, 70, 100, 0);

    let stats = render(
        &document(&[
            long_ends_before_window,
            short_ends_before_window,
            short_key_out,
            long_in_group,
            short_in_group,
            short_later,
            long_later,
            short_last,
        ]),
        600,
        1200,
        60,
        70,
    );
    assert_eq!(
        stats.draw_order,
        vec![
            long_in_group,
            short_in_group,
            short_later,
            long_later,
            short_last
        ],
        "组内越界/跨出窗口的音符不得绘制，且后续 tick 的音符不得漏画"
    );
    assert_eq!(
        stats.visible_notes, 5,
        "只有落在窗口与 key 范围内的音符计数"
    );
}

/// `u32::MAX` 的组：`saturating_add(1)` 不能溢出，组查询只取该 tick（不漏、不越界）。
///
/// 生产路径下 `tick_hi == u32::MAX` 时窗口流的 `window_range` 同样会被
/// `saturating_add` 截断（本修复范围外的既有边界），这里直接驱动组查询锁死
/// 它自身的边界行为：区间退化为空时必须回退到文档末尾。
#[test]
fn tick_group_at_u32_max_covers_only_that_tick() {
    let previous_tick = NoteEvent::new(u32::MAX - 1, u32::MAX, 60, 100, 0);
    let max_tick = NoteEvent::new(u32::MAX, u32::MAX, 62, 100, 0);
    let document = Arc::new(document(&[previous_tick, max_tick]));
    let job = job_for(Arc::clone(&document), u32::MAX, u32::MAX, 60, 70);
    let mut buffer = BandBuffer::default();
    let mut stats = BandStats::default();
    draw_tick_group(
        &job,
        &mut buffer,
        0,
        document.track_notes(0),
        u32::MAX,
        &mut stats,
    );
    assert_eq!(
        stats.draw_order,
        vec![max_tick],
        "u32::MAX 组只取该 tick（不得溢出、不得把前一个 tick 带进来）"
    );
}

/// 组查询只在 tie 时发生：无长音符时窗口流照常逐条绘制（常路径零回归）。
#[test]
fn window_only_stream_keeps_tick_order() {
    let n1 = NoteEvent::new(100, 200, 60, 100, 0);
    let n2 = NoteEvent::new(100, 300, 62, 100, 0);
    let n3 = NoteEvent::new(200, 300, 64, 100, 0);
    let stats = render(&document(&[n1, n2, n3]), 0, 400, 60, 70);
    assert_eq!(stats.draw_order, vec![n1, n2, n3]);
    assert_eq!(stats.visible_notes, 3);
}
