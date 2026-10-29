//! 区域着色点与网格测试

use super::*;

fn square(lo: f32, hi: f32) -> Vec<(f32, f32)> {
    vec![(lo, lo), (hi, lo), (hi, hi), (lo, hi)]
}

#[test]
fn test_area_spot_accepts_boundaries() {
    for colour in [0, 1, 15, AREA_COLOUR_MAX] {
        AreaSpot {
            u: 0.0,
            v: 1.0,
            colour,
        }
        .validate()
        .unwrap_or_else(|e| panic!("边界色号 {colour} 应通过校验: {e}"));
    }
}

#[test]
fn test_area_spot_rejects_out_of_range_coords() {
    for (u, v) in [(-0.01, 0.5), (1.01, 0.5), (0.5, -1.0), (0.5, 2.0)] {
        let err = AreaSpot { u, v, colour: 1 }
            .validate()
            .expect_err("越界坐标应被拒绝");
        assert!(format!("{err}").contains("坐标越界"), "实际错误：{err}");
    }
}

#[test]
fn test_area_spot_rejects_non_finite_coords() {
    for (u, v) in [(f32::NAN, 0.5), (0.5, f32::INFINITY)] {
        let err = AreaSpot { u, v, colour: 0 }
            .validate()
            .expect_err("非有限坐标应被拒绝");
        assert!(format!("{err}").contains("非有限"), "实际错误：{err}");
    }
}

#[test]
fn test_area_spot_rejects_bad_colour() {
    let err = AreaSpot {
        u: 0.5,
        v: 0.5,
        colour: AREA_COLOUR_MAX + 1,
    }
    .validate()
    .expect_err("色号越界应被拒绝");
    assert!(format!("{err}").contains("色号越界"), "实际错误：{err}");
}

#[test]
fn test_most_colours_respects_channel_10() {
    assert_eq!(most_colours(false), 15);
    assert_eq!(most_colours(true), 16);
}

#[test]
fn test_rasterize_square_and_paint_region() {
    let mut map = AreaMap::rasterize(&[square(0.2, 0.8)], false);
    assert_eq!(
        map.area_at(0.5, 0.5),
        AREA_UNASSIGNED,
        "闭合路径内部应未着色"
    );
    assert_eq!(map.area_at(0.05, 0.5), AREA_OUTSIDE, "路径外部应为 outside");
    assert!(map.paint_area(0.5, 0.5, 3), "内部点应能着 colour=3");
    assert_eq!(map.area_at(0.5, 0.5), 3);
    assert_eq!(map.area_at(0.1, 0.5), AREA_OUTSIDE, "外部不应被着色");
    assert!(!map.paint_area(0.5, 0.5, 3), "同色重复着色应返回 false");
}

#[test]
fn test_paint_empty_then_recolour() {
    let mut map = AreaMap::rasterize(&[square(0.2, 0.8)], false);
    assert!(map.paint_area(0.5, 0.5, 0), "应能标为空");
    assert_eq!(map.area_at(0.5, 0.5), 0);
    assert!(map.paint_area(0.5, 0.5, 5), "空区域应能重新着色");
    assert_eq!(map.area_at(0.5, 0.5), 5);
}

#[test]
fn test_even_odd_hole_is_outside() {
    let paths = [square(0.1, 0.9), square(0.4, 0.6)];
    let map = AreaMap::rasterize(&paths, false);
    assert_eq!(
        map.area_at(0.5, 0.5),
        AREA_OUTSIDE,
        "奇偶规则下内环应为空洞"
    );
    assert_eq!(
        map.area_at(0.2, 0.5),
        AREA_UNASSIGNED,
        "外环与内环之间应为内部"
    );
    assert!(!map.clone().paint_area(0.5, 0.5, 3), "空洞点不可着色");
}

#[test]
fn test_union_fills_hole() {
    let paths = [square(0.1, 0.9), square(0.4, 0.6)];
    let map = AreaMap::rasterize(&paths, true);
    assert_eq!(
        map.area_at(0.5, 0.5),
        AREA_UNASSIGNED,
        "非零环绕下内环仍属于内部"
    );
}

#[test]
fn test_area_at_rejects_non_finite_and_out_of_range() {
    let map = AreaMap::new();
    for (u, v) in [
        (f32::NAN, 0.5),
        (0.5, f32::INFINITY),
        (-0.1, 0.5),
        (1.1, 0.5),
    ] {
        assert_eq!(map.area_at(u, v), AREA_OUTSIDE);
    }
}

#[test]
fn test_paint_rejects_invalid_colour_and_outside_point() {
    let mut map = AreaMap::rasterize(&[square(0.2, 0.8)], false);
    assert!(!map.paint_area(0.5, 0.5, ALL_COLOURS + 1));
    assert!(!map.paint_area(0.05, 0.5, 3));
}

#[test]
fn test_empty_map_has_no_inside() {
    let map = AreaMap::new();
    assert_eq!(map.area_at(0.5, 0.5), AREA_OUTSIDE);
    assert_eq!(map.cells.len(), 1024 * 1024);
}
