//! Gate Range 纯算法测试

use super::*;

fn straight() -> Vec<[f32; 2]> {
    vec![[0.0, 0.0], [1.0, 1.0]]
}

#[test]
fn test_gate_at_straight_graph() {
    let graph = straight();
    assert_eq!(gate_at(&graph, 30, 120, 0.0, false), 30);
    assert_eq!(gate_at(&graph, 30, 120, 0.5, false), 75);
    assert_eq!(gate_at(&graph, 30, 120, 1.0, false), 120);
}

#[test]
fn test_steps_cover_unit_and_gates_rise() {
    let steps = steps_of(&straight(), 30, 120, false);
    assert!(!steps.is_empty());
    assert_eq!(steps[0].0, 0.0);
    assert_eq!(steps.last().expect("应有末段").1, 1.0);
    assert_eq!(steps.first().expect("应有首段").2, 30);
    assert_eq!(steps.last().expect("应有末段").2, 120);
    for pair in steps.windows(2) {
        assert!(pair[1].2 >= pair[0].2, "直线 graph 门长应单调不减");
        assert!((pair[0].1 - pair[1].0).abs() < 1e-6, "steps 应首尾相接");
    }
}

#[test]
fn test_range_squares_are_back_to_back() {
    let squares = range_squares(0, 960, 30, 120, &straight(), false);
    assert!(!squares.is_empty());
    assert_eq!(squares[0].0, 0);
    assert!(squares.last().expect("应有末片").1 >= 960);
    for pair in squares.windows(2) {
        assert_eq!(pair[0].1, pair[1].0, "门必须背靠背，无缝隙");
    }
}

#[test]
fn test_keys_and_keys_down_are_equivalent() {
    let graph = straight();
    let low = keys_gate(60, 60, 64, 30, 120, &graph, false);
    let high = keys_gate(64, 60, 64, 30, 120, &graph, false);
    assert_eq!(low, 30);
    assert_eq!(high, 120);

    let params = GateRangeParams {
        dir: RangeDir::KeysDown,
        graph: graph.clone(),
        ..GateRangeParams::default()
    };
    let (dir, reversed) = normalize(&params);
    assert_eq!(dir, RangeDir::Keys);
    assert_eq!(
        keys_gate(60, 60, 64, 30, 120, &reversed, false),
        high,
        "keys_down 低行应等于 keys 高行"
    );
}

#[test]
fn test_halves_snap_to_powers_of_two() {
    let graph = straight();
    let gate = gate_at(&graph, 30, 120, 0.5, true);
    assert_eq!(gate, 60, "75 最近吸附到 60");
    assert_eq!(gate_at(&graph, 30, 120, 1.0, true), 120);
    assert_eq!(gate_at(&graph, 30, 120, 0.0, true), 30);
}

#[test]
fn test_fit_clamps_edges() {
    let mut squares = range_squares(100, 500, 30, 120, &straight(), false);
    fit_squares(&mut squares, 100, 500);
    assert_eq!(squares[0].0, 100);
    assert_eq!(squares.last().expect("应有末片").1, 500);
}

#[test]
fn test_custom_graph_interpolates() {
    let graph = vec![[0.0, 0.0], [0.5, 1.0], [1.0, 0.0]];
    assert_eq!(gate_at(&graph, 30, 120, 0.0, false), 30);
    assert_eq!(gate_at(&graph, 30, 120, 0.5, false), 120);
    assert_eq!(gate_at(&graph, 30, 120, 1.0, false), 30);
}
