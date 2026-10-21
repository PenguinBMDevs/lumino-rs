//! Between 曲线测试

use super::*;

#[test]
fn test_empty_graph_is_linear() {
    let set = BetweenSet::default();
    assert!((graph_at(&set, 0.25) - 0.25).abs() < 1e-6);
}

#[test]
fn test_graph_interpolates_and_clamps() {
    let set = BetweenSet {
        graph: vec![[0.0, 0.0], [0.5, 1.0], [1.0, 0.0]],
        ..BetweenSet::default()
    };
    assert!((graph_at(&set, 0.5) - 1.0).abs() < 1e-6);
    assert!((graph_at(&set, 0.75) - 0.5).abs() < 1e-6);
    assert_eq!(graph_at(&set, -1.0), 0.0);
    assert_eq!(graph_at(&set, 2.0), 0.0);
}
