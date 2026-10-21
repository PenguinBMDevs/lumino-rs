//! Slice 几何测试

use super::*;

fn square() -> Vec<Pt> {
    vec![
        (0.0, 0.0),
        (10.0, 0.0),
        (10.0, 10.0),
        (0.0, 10.0),
        (0.0, 0.0),
    ]
}

#[test]
fn test_vertical_crossings_on_closed_square() {
    let hits = crossings(&square(), (5.0, -1.0), (5.0, 11.0), true);
    assert_eq!(hits.len(), 2);
    assert!((hits[0].0.0 - 5.0).abs() < 1e-4);
    assert!((hits[0].0.1 - 0.0).abs() < 1e-4);
    assert!((hits[1].0.1 - 10.0).abs() < 1e-4);
}

#[test]
fn test_segment_limits_and_whole_line() {
    let path = square();
    let outside = crossings(&path, (5.0, 20.0), (5.0, 30.0), false);
    assert!(outside.is_empty(), "线段不相交应为空");
    let infinite = crossings(&path, (5.0, 20.0), (5.0, 30.0), true);
    assert_eq!(infinite.len(), 2, "无限长直线应穿透");
}

#[test]
fn test_collinear_does_not_panic() {
    let path = vec![(0.0, 0.0), (10.0, 0.0)];
    let hits = crossings(&path, (-5.0, 0.0), (15.0, 0.0), true);
    assert!(hits.len() <= 2);
}

#[test]
fn test_inside_even_odd_with_hole() {
    let outer = square();
    let hole = vec![(3.0, 3.0), (7.0, 3.0), (7.0, 7.0), (3.0, 7.0), (3.0, 3.0)];
    assert!(inside(std::slice::from_ref(&outer), (5.0, 5.0)));
    assert!(!inside(std::slice::from_ref(&outer), (15.0, 5.0)));
    assert!(!inside(&[outer, hole], (5.0, 5.0)), "洞内应为空");
}

#[test]
fn test_cut_loops_returns_two_closed_halves() {
    let (left, right) = cut_loops(&[square()], (5.0, -1.0), (5.0, 11.0)).expect("应能切");
    let left = &left[0];
    let right = &right[0];
    assert_eq!(left[0], *left.last().expect("非空"), "左半应闭合");
    assert_eq!(right[0], *right.last().expect("非空"), "右半应闭合");
    assert!(left.iter().any(|p| (p.0 - 5.0).abs() < 1e-4));
    assert!(right.iter().any(|p| (p.0 - 5.0).abs() < 1e-4));
}

#[test]
fn test_cut_loops_outside_returns_none() {
    assert!(cut_loops(&[square()], (20.0, -1.0), (20.0, 11.0)).is_none());
    assert!(cut_loops(&[vec![(0.0, 0.0), (1.0, 0.0)]], (5.0, -1.0), (5.0, 1.0)).is_none());
}
