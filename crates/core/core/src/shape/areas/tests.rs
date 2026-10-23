//! 区域着色点测试

use super::*;

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
