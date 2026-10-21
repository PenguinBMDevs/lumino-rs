//! Copy path 落点测试

use super::*;

fn run() -> Run {
    Run {
        a: (0.0, 60.0),
        b: (100.0, 60.0),
    }
}

#[test]
fn test_count_evenly_spaced() {
    let places = place_on_run(&run(), 3, Speed::Even, [100.0, 100.0]);
    assert_eq!(places.len(), 3);
    let xs = places.iter().map(|p| p.corner.0).collect::<Vec<_>>();
    assert_eq!(xs, vec![25.0, 50.0, 75.0]);
    assert_eq!(places[0].scale, [1.0, 1.0]);
}

#[test]
fn test_size_lerps_to_end() {
    let places = place_on_run(&run(), 2, Speed::Even, [100.0, 50.0]);
    assert!((places[1].scale[1] - 0.5).abs() < 1e-6);
}

#[test]
fn test_up_and_down_are_monotonic() {
    let up = place_on_run(&run(), 4, Speed::Up, [100.0, 100.0]);
    let down = place_on_run(&run(), 4, Speed::Down, [100.0, 100.0]);
    let up_gaps = up.windows(2).map(|p| p[1].t - p[0].t).collect::<Vec<_>>();
    assert!(up_gaps.windows(2).all(|pair| pair[0] < pair[1]));
    let down_gaps = down.windows(2).map(|p| p[1].t - p[0].t).collect::<Vec<_>>();
    assert!(down_gaps.windows(2).all(|pair| pair[0] > pair[1]));
}

#[test]
fn test_corner_a_up_down() {
    let rect = ShapeRect::normalized(0.0, 60.0, 100.0, 64.0);
    assert_eq!(corner_a(&rect, Dir::Up), (0.0, 60.0));
    assert_eq!(corner_a(&rect, Dir::Down), (0.0, 64.0));
}
