//! Step 5b：脏区透传（`CpuOnionOutput::dirty` → `CpuOnionFrame::dirty_rect`）。
//!
//! gfx 的局部上传只认帧上的 `dirty_rect`，因此这里锁死三件事：
//! 区域重绘必须上报**实际矩形**、整帧重绘必须是 `None`、复用帧必须把旧脏区清掉。

use lumino_cpu_onion::CpuOnionRect;
use lumino_editor_state::CpuOnionDirty;
use lumino_gfx::CpuOnionUploadRect;
use lumino_midi_model::NoteEvent;

use super::{HEIGHT, MUTED, WIDTH, change, dirty, document, key, note_row, tracks};
use crate::host::render::cpu_onion::CpuOnionState;

/// 把 CPU 光栅矩形转成 gfx 帧内矩形（测试侧独立表达，避免直接调用被测映射）。
fn upload_rect(rect: CpuOnionRect) -> CpuOnionUploadRect {
    CpuOnionUploadRect::new(rect.x, rect.y, rect.width, rect.height)
}

/// 洋葱轨单笔编辑 → 区域重绘 → `dirty = Some(实际矩形)` 且帧上脏区一致。
#[test]
fn region_redraw_reports_dirty_rect() {
    let before = document();
    let mut after = before.clone();
    let old = NoteEvent::new(10, 110, 121, 100, 0);
    let new = NoteEvent::new(10, 110, 122, 100, 0);
    after.remove_note(1, 0);
    after.insert_note(1, new);

    let mut state = CpuOnionState::default();
    let first = state.update(key(1, 0), &before, &MUTED, None, None);
    assert!(first.rebuilt, "首帧必须重绘");
    assert_eq!(first.dirty, None, "整帧重绘不得上报脏区");
    assert_eq!(
        first.frame.expect("首帧应生成").dirty_rect,
        None,
        "整帧重绘的帧不得携带脏区"
    );

    let output = state.update(
        key(2, 0),
        &after,
        &MUTED,
        Some(dirty(vec![change(1, Some(old), Some(new))])),
        Some(&tracks(&[1])),
    );
    assert!(output.rebuilt, "洋葱轨变化必须重绘");

    let region = output.dirty.expect("区域重绘必须上报脏区");
    let frame = output.frame.expect("应生成帧");
    assert_eq!(
        frame.dirty_rect,
        Some(upload_rect(region)),
        "帧上的脏区必须与输出一致（gfx 只读帧字段）"
    );

    // 脏区必须真的覆盖变化像素，且明显小于整帧（否则 plan 会走整帧）
    let frame_area = u64::from(WIDTH as u32) * u64::from(HEIGHT as u32);
    assert!(
        region.area() * 4 < frame_area,
        "区域重绘面积必须远小于整帧：{region:?}"
    );
    for note_key in [121u16, 122u16] {
        let row = note_row(note_key);
        assert!(
            region.y <= row && row < region.y + region.height,
            "脏区必须覆盖键 {note_key} 所在行 {row}：{region:?}"
        );
    }
    assert!(
        region.x <= 60 && 60 < region.x + region.width,
        "脏区必须覆盖音符横向范围（tick 10..110）：{region:?}"
    );
    // 帧内坐标必须完整落在帧内：越界会被 gfx 拒绝并回退全量上传
    assert_eq!(
        region.clamp_to(frame.width, frame.height),
        Some(region),
        "脏区不得越界（否则局部上传退化为全量）"
    );
}

/// 整帧重绘（`dirty.full`）→ `dirty = None`，帧上无脏区。
#[test]
fn full_redraw_reports_no_dirty_rect() {
    let before = document();
    let mut after = before.clone();
    after.insert_note(1, NoteEvent::new(300, 400, 118, 100, 0));

    let mut state = CpuOnionState::default();
    state.update(key(1, 0), &before, &MUTED, None, None);

    let output = state.update(
        key(2, 0),
        &after,
        &MUTED,
        Some(CpuOnionDirty {
            full: true,
            changes: Vec::new(),
            ranges: Vec::new(),
        }),
        Some(&tracks(&[1])),
    );
    assert!(output.rebuilt, "整帧兜底必须重绘");
    assert_eq!(output.dirty, None, "整帧重绘必须按全量语义上报");
    assert_eq!(
        output.frame.expect("应生成帧").dirty_rect,
        None,
        "整帧重绘的帧不得携带脏区"
    );
}

/// 复用帧（指纹不变）→ `dirty = None`，且**旧脏区被清掉**（含驻留副本）。
#[test]
fn reused_frame_clears_stale_dirty_rect() {
    let before = document();
    let mut after = before.clone();
    let old = NoteEvent::new(10, 110, 121, 100, 0);
    let new = NoteEvent::new(10, 110, 122, 100, 0);
    after.remove_note(1, 0);
    after.insert_note(1, new);

    let mut state = CpuOnionState::default();
    state.update(key(1, 0), &before, &MUTED, None, None);
    let redrawn = state.update(
        key(2, 0),
        &after,
        &MUTED,
        Some(dirty(vec![change(1, Some(old), Some(new))])),
        Some(&tracks(&[1])),
    );
    assert!(redrawn.dirty.is_some(), "前置：这一帧确实是区域重绘");
    let version = redrawn.frame.expect("应生成帧").version;

    // 指纹不变（同一代次/相机/静音）→ 复用，不重绘
    let reused = state.update(key(2, 0), &after, &MUTED, None, None);
    assert!(!reused.rebuilt, "没有新输入时必须复用上一帧");
    assert_eq!(reused.dirty, None, "复用帧不得上报脏区");
    let frame = reused.frame.expect("应复用帧");
    assert_eq!(frame.version, version, "复用帧版本不变（gfx 侧 Reuse）");
    assert_eq!(
        frame.dirty_rect, None,
        "复用帧不得携带上一帧的旧脏区（否则被误当成本帧增量）"
    );
    assert_eq!(
        state.frame().and_then(|frame| frame.dirty_rect),
        None,
        "驻留副本的脏区也必须清掉，避免下一次复用继续带毒"
    );
}
