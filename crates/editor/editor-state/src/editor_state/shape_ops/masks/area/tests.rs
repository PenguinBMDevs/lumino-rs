//! Mask 区域代数测试

use super::*;

fn note(tick: f32, key: u16, length: f32) -> ShapeNote {
    ShapeNote {
        track: 0,
        tick,
        key,
        length,
        velocity: 100,
        channel: 0,
    }
}

fn rect_polygon() -> Vec<(f32, f32)> {
    vec![(0.0, 60.0), (480.0, 60.0), (480.0, 64.0), (0.0, 64.0)]
}

#[test]
fn test_area_merges_and_subtracts() {
    let a = Area::new([
        RowSpan {
            key: 60,
            start: 0.0,
            end: 100.0,
        },
        RowSpan {
            key: 60,
            start: 100.0,
            end: 200.0,
        },
        RowSpan {
            key: 61,
            start: 0.0,
            end: 50.0,
        },
    ]);
    assert_eq!(a.len(), 2, "相接段应合并");
    assert_eq!(a.spans_on(60)[0].end, 200.0);

    let b = Area::new([RowSpan {
        key: 60,
        start: 50.0,
        end: 150.0,
    }]);
    let cut = a.minus(&b);
    assert_eq!(cut.spans_on(60).len(), 2);
    assert_eq!(cut.spans_on(60)[0].start, 0.0);
    assert_eq!(cut.spans_on(60)[0].end, 50.0);
    assert_eq!(cut.spans_on(60)[1].start, 150.0);
    assert_eq!(cut.spans_on(60)[1].end, 200.0);

    let both = a.common(&b);
    assert_eq!(both.len(), 1);
    assert_eq!(both.spans_on(60)[0].start, 50.0);
    assert_eq!(both.spans_on(60)[0].end, 150.0);
}

#[test]
fn test_inside_rectangle_fills_rows_not_boundary() {
    let area = inside_area(&rect_polygon(), 128);
    assert_eq!(area.len(), 4, "60..64 只应填 60/61/62/63 四行");
    for key in 60..64 {
        let spans = area.spans_on(key);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].start, 0.0);
        assert_eq!(spans[0].end, 480.0);
    }
    assert!(area.spans_on(64).is_empty(), "上边界行不应被填");
}

#[test]
fn test_inside_triangle_narrows_toward_apex() {
    let area = inside_area(&[(0.0, 60.0), (480.0, 60.0), (240.0, 64.0)], 128);
    assert_eq!(area.len(), 4);
    let base = area.spans_on(60)[0];
    let top = area.spans_on(63)[0];
    assert!(
        top.end - top.start < base.end - base.start,
        "越靠顶点应越窄"
    );
}

#[test]
fn test_band_is_round_and_zoom_scales_ticks() {
    let notes = vec![note(0.0, 60, 100.0)];
    let area = band_area(&notes, 2.0, 30.0, 128);
    assert_eq!(area.len(), 3, "半径 1 key：本行 + 上下各一行");
    let own = area.spans_on(60)[0];
    assert_eq!((own.start, own.end), (-30.0, 130.0));
    let above = area.spans_on(59)[0];
    assert_eq!((above.start, above.end), (0.0, 100.0), "边缘行横向 reach=0");

    let zoomed = band_area(&notes, 2.0, 60.0, 128);
    assert_eq!(zoomed.spans_on(60)[0].start, -60.0);
    assert_eq!(zoomed.spans_on(60)[0].end, 160.0, "k 翻倍 ⇒ tick 宽翻倍");
}

#[test]
fn test_band_size_zero_is_just_notes() {
    let notes = vec![note(10.0, 60, 50.0)];
    let area = band_area(&notes, 0.0, 30.0, 128);
    assert_eq!(area.len(), 1);
    assert_eq!(
        (area.spans_on(60)[0].start, area.spans_on(60)[0].end),
        (10.0, 60.0)
    );
}

#[test]
fn test_base_solid_uses_inside_only() {
    let notes = vec![note(0.0, 60, 10.0)];
    let polygon = rect_polygon();
    let (base, own) = base_areas(&MaskAreaInput {
        notes: &notes,
        polygon: Some(&polygon),
        solid: true,
        show_line: false,
        own: true,
        size_keys: 10.0,
        tpk: 30.0,
        nkeys: 128,
    });
    assert_eq!(base.len(), 4, "Fill/Spam 取整个内部");
    assert!(own.is_empty(), "solid 不做 own 边框避让");
}

#[test]
fn test_base_line_uses_band_not_inside() {
    let notes = vec![note(0.0, 60, 100.0)];
    let polygon = rect_polygon();
    let (base, _) = base_areas(&MaskAreaInput {
        notes: &notes,
        polygon: Some(&polygon),
        solid: false,
        show_line: true,
        own: true,
        size_keys: 0.0,
        tpk: 30.0,
        nkeys: 128,
    });
    assert_eq!(base.len(), 1, "line 模式只取沿线带");
    assert_eq!(base.spans_on(60)[0].end, 100.0);
    assert!(base.spans_on(61).is_empty());
}

#[test]
fn test_base_inside_includes_band_and_own() {
    // 放在 61 行：band 的 60/61/62 全在内部 60..64 内，合并后仍是四行
    let notes = vec![note(100.0, 61, 100.0)];
    let polygon = rect_polygon();
    let (base, own) = base_areas(&MaskAreaInput {
        notes: &notes,
        polygon: Some(&polygon),
        solid: false,
        show_line: false,
        own: true,
        size_keys: 2.0,
        tpk: 30.0,
        nkeys: 128,
    });
    assert_eq!(base.len(), 4, "内部 + band 合并后仍是四行");
    assert_eq!(own.len(), 1);
    assert_eq!(own.spans_on(61)[0].start, 100.0);
    assert_eq!(own.spans_on(61)[0].end, 200.0);
}

#[test]
fn test_own_false_is_a_pure_cutter() {
    let notes = vec![note(100.0, 61, 100.0)];
    let polygon = rect_polygon();
    let (_, own) = base_areas(&MaskAreaInput {
        notes: &notes,
        polygon: Some(&polygon),
        solid: false,
        show_line: false,
        own: false,
        size_keys: 2.0,
        tpk: 30.0,
        nkeys: 128,
    });
    assert!(own.is_empty(), "own=false 时不应有边框避让区域");
}

#[test]
fn test_intersects_respects_key_and_time() {
    let area = Area::new([RowSpan {
        key: 60,
        start: 100.0,
        end: 200.0,
    }]);
    assert!(area.intersects(60, 150.0, 160.0));
    assert!(!area.intersects(61, 150.0, 160.0));
    assert!(!area.intersects(60, 200.0, 220.0));
}
