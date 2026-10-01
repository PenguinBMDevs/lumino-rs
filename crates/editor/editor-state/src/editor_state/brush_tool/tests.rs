//! 画刷矢量笔画状态与覆盖计算测试
//!
//! 重点回归「断墨」：快速拖动（一帧跨多格）时覆盖集必须连续无洞。

use super::cov::{self, CoveredCell};
use super::*;
use crate::editor_state::EditorState;
use lumino_core::Tool;
use std::collections::{HashSet, VecDeque};

/// 单格精度（模拟 1/16 音符 @ PPQ 1920 之外的自定义精度，不影响断言）
const SNAP: f32 = 240.0;

/// 覆盖集是否 4 连通（相邻格共边）——「无洞」的强不变式
fn is_4_connected(cells: &[CoveredCell]) -> bool {
    if cells.is_empty() {
        return true;
    }
    let set: HashSet<CoveredCell> = cells.iter().copied().collect();
    let Some(&start) = cells.first() else {
        return true;
    };
    let mut seen: HashSet<CoveredCell> = HashSet::new();
    let mut queue = VecDeque::new();
    seen.insert(start);
    queue.push_back(start);
    while let Some((i, j)) = queue.pop_front() {
        let neighbors = [
            (i + 1, j),
            (i - 1, j),
            (i, j.saturating_add(1)),
            (i, j.saturating_sub(1)),
        ];
        for n in neighbors {
            if set.contains(&n) && seen.insert(n) {
                queue.push_back(n);
            }
        }
    }
    seen.len() == set.len()
}

// ── 覆盖计算（断墨修复） ─────────────────────────

#[test]
fn test_cover_single_point() {
    let cells = cov::cover_cells(&[(480.0, 60.0)], SNAP);
    assert_eq!(cells, vec![(2, 60)], "单点笔画 = 所在格");
}

#[test]
fn test_cover_fast_drag_covers_every_cell() {
    // 一帧从第 0 格跨到第 10 格（旧实现只会盖首尾两格 → 中间 9 格空洞）
    let cells = cov::cover_cells(&[(0.0, 60.0), (10.0 * SNAP, 60.0)], SNAP);
    let ticks: Vec<i64> = cells.iter().map(|c| c.0).collect();
    assert_eq!(
        ticks,
        (0..=10).collect::<Vec<i64>>(),
        "水平快拖必须逐格覆盖、顺序连续"
    );
    assert!(cells.iter().all(|c| c.1 == 60));
}

#[test]
fn test_cover_steep_diagonal_is_connected() {
    // 陡峭斜线：X 格差 4、Y 键差 6（旧实现每帧只盖 1 格 → 断墨最严重的场景）
    let points = [(0.0, 60.0), (4.0 * SNAP, 66.0)];
    let cells = cov::cover_cells(&points, SNAP);
    assert!(cells.len() >= 7, "覆盖格数不少于主步进数：{}", cells.len());
    assert!(is_4_connected(&cells), "斜线覆盖集必须 4 连通（无洞）");
    assert!(cells.contains(&(0, 60)), "含起点格");
    assert!(cells.contains(&(4, 66)), "含终点格");
}

#[test]
fn test_cover_shallow_diagonal_is_connected() {
    let points = [(0.0, 60.0), (12.0 * SNAP, 63.0)];
    let cells = cov::cover_cells(&points, SNAP);
    assert!(is_4_connected(&cells), "缓斜线覆盖集必须 4 连通");
    assert!(cells.contains(&(0, 60)));
    assert!(cells.contains(&(12, 63)));
}

#[test]
fn test_cover_vertical_segment() {
    // 同一 tick 格内上下移动：覆盖该格全部 key（预览/生成都是一条竖线）
    let cells = cov::cover_cells(&[(0.0, 60.0), (0.0, 64.0)], SNAP);
    assert_eq!(
        cells,
        vec![(0, 60), (0, 61), (0, 62), (0, 63), (0, 64)],
        "竖直段逐 key 覆盖"
    );
}

#[test]
fn test_cover_reverse_drag() {
    // 反向拖动（从右往左、从高往低）同样连续
    let points = [(10.0 * SNAP, 70.0), (0.0, 60.0)];
    let cells = cov::cover_cells(&points, SNAP);
    assert!(cells.contains(&(10, 70)));
    assert!(cells.contains(&(0, 60)));
    assert!(is_4_connected(&cells));
}

#[test]
fn test_cover_dedup_and_duplicate_points() {
    let cells = cov::cover_cells(&[(0.0, 60.0), (0.0, 60.0), (0.0, 60.0)], SNAP);
    assert_eq!(cells, vec![(0, 60)], "重合点/重复段去重");

    let mut seen = HashSet::new();
    for c in cov::cover_cells(&[(0.0, 60.0), (5.0 * SNAP, 65.0)], SNAP) {
        assert!(seen.insert(c), "覆盖集内不得有重复格");
    }
}

#[test]
fn test_cover_multi_segment_polyline_connected() {
    // 折线三拐点：整体覆盖仍是一条连通笔画
    let points = [
        (0.0, 60.0),
        (3.0 * SNAP, 60.0),
        (3.0 * SNAP, 64.0),
        (7.0 * SNAP, 64.0),
    ];
    let cells = cov::cover_cells(&points, SNAP);
    assert!(is_4_connected(&cells), "折线整体 4 连通");
    assert!(cells.contains(&(0, 60)));
    assert!(cells.contains(&(7, 64)));
}

#[test]
fn test_cover_is_independent_of_sampling_density() {
    // 同一几何路径：稀疏采样（两点）与密采样（2001 点）必须给出同一覆盖集。
    // 这是"断墨修复不依赖采样频率"的正确性口径，也是性能口径
    //（覆盖成本 O(覆盖格数)，与采样点数无关）。
    let sparse = cov::cover_cells(&[(0.0, 60.0), (100.0 * SNAP, 64.0)], SNAP);
    let dense_points: Vec<(f32, f32)> = (0..=2000)
        .map(|i| {
            let t = i as f32 / 2000.0;
            (t * 100.0 * SNAP, 60.0 + t * 4.0)
        })
        .collect();
    let dense = cov::cover_cells(&dense_points, SNAP);

    let sparse_set: HashSet<CoveredCell> = sparse.iter().copied().collect();
    let dense_set: HashSet<CoveredCell> = dense.iter().copied().collect();
    for cell in &sparse_set {
        assert!(
            dense_set.contains(cell),
            "密采样不得丢失稀疏采样覆盖的格：{cell:?}"
        );
    }
    assert_eq!(dense.len(), dense_set.len(), "覆盖集内无重复格");
    assert!(is_4_connected(&dense));
}

#[test]
fn test_cover_negative_tick_maps_to_negative_cell() {
    // 反向拖到 0 之前：格索引为负（上层钳制，不在本层静默改写数据）
    let cells = cov::cover_cells(&[(0.0, 60.0), (-2.0 * SNAP, 60.0)], SNAP);
    let expected: Vec<CoveredCell> = vec![(0, 60), (-1, 60), (-2, 60)];
    assert_eq!(cells, expected, "负向拖动逐格覆盖");
}

#[test]
fn test_cover_zero_snap_falls_back_without_panic() {
    let cells = cov::cover_cells(&[(0.0, 60.0), (3.0, 60.0)], 0.0);
    assert!(!cells.is_empty(), "非法精度退化为 1，不得 panic/NaN");

    let cells = cov::cover_cells(&[(0.0, 60.0), (3.0, 60.0)], f32::NAN);
    assert!(!cells.is_empty(), "NaN 精度同样退化处理");
}

#[test]
fn test_cover_key_clamped_to_midi_range() {
    let cells = cov::cover_cells(&[(0.0, 300.0)], SNAP);
    assert_eq!(cells, vec![(0, 255)], "key 钳制到 255");
}

// ── 层展开 ─────────────────────────

#[test]
fn test_expand_layers_upward_from_base_key() {
    let cells = vec![(0, 60), (1, 60)];
    let mut out = Vec::new();
    cov::expand_layers(&cells, 3, &mut out);
    assert_eq!(
        out,
        vec![
            ((0, 60), 0),
            ((0, 61), 1),
            ((0, 62), 2),
            ((1, 60), 0),
            ((1, 61), 1),
            ((1, 62), 2)
        ],
        "每格向上展开 thickness 层，并带回层级（用于解析音轨/颜色）"
    );
}

#[test]
fn test_expand_layers_clamped_at_max_key() {
    let cells = vec![(0, 254)];
    let mut out = Vec::new();
    cov::expand_layers(&cells, 3, &mut out);
    assert_eq!(out, vec![((0, 254), 0), ((0, 255), 1)], "超过 255 截断");
}

#[test]
fn test_expand_layers_zero_thickness() {
    let mut out = Vec::new();
    cov::expand_layers(&[(0, 60)], 0, &mut out);
    assert!(out.is_empty());
}

#[test]
fn test_cell_tick_roundtrip() {
    assert_eq!(cov::cell_tick(3, SNAP), 3.0 * SNAP);
    assert_eq!(cov::tick_cell(3.0 * SNAP, SNAP), 3);
}

// ── 笔画状态 ─────────────────────────

#[test]
fn test_begin_stroke_and_dedupe_points() {
    let mut state = BrushToolState::default();
    assert!(!state.has_pending());
    assert!(!state.is_active());

    let idx = state.begin_stroke((0.0, 60.0), 1);
    assert_eq!(idx, 0);
    assert!(state.is_drawing() && state.is_active());
    assert!(state.has_pending());

    assert!(!state.push_point((0.0, 60.0)), "与末点重合不追加");
    assert!(state.push_point((120.0, 61.0)));
    assert_eq!(state.strokes[0].points.len(), 2);

    state.finish_stroke();
    assert!(!state.is_active(), "松手退出交互态");
    assert!(state.has_pending(), "松手后笔画仍待确认");
}

#[test]
fn test_multiple_strokes_coexist() {
    let mut state = BrushToolState::default();
    state.begin_stroke((0.0, 60.0), 1);
    state.finish_stroke();
    state.begin_stroke((480.0, 64.0), 1);
    state.finish_stroke();
    assert_eq!(state.strokes.len(), 2, "空白处按下开始新笔画，不清空已有");
}

#[test]
fn test_drag_snaps_key_to_single_key() {
    let mut state = BrushToolState::default();
    state.begin_stroke((0.0, 60.0), 1);
    state.push_point((240.0, 60.0));
    state.finish_stroke();

    assert!(state.begin_drag(0, (100.0, 60.0)));
    assert!(state.is_dragging());
    // X 自由（+37.5 tick）、Y 按单个 key 吸附（+1.6 → +2 key）
    state.drag_to((137.5, 61.6));
    let pts = &state.strokes[0].points;
    assert!((pts[0].0 - 37.5).abs() < 1e-3, "X 向自由：{:?}", pts[0]);
    assert_eq!(pts[0].1, 62.0, "Y 向按单 key 吸附");
    assert_eq!(pts[1].1, 62.0);

    state.end_drag();
    assert!(!state.is_active());
    assert!(state.has_pending(), "拖动后仍是待确认笔画");
}

#[test]
fn test_drag_is_relative_to_original_not_accumulated() {
    let mut state = BrushToolState::default();
    state.begin_stroke((1000.0, 60.0), 1);
    state.finish_stroke();
    state.begin_drag(0, (1000.0, 60.0));

    state.drag_to((1300.0, 63.9));
    state.drag_to((1100.0, 61.2));
    let p = state.strokes[0].points[0];
    assert!((p.0 - 1100.0).abs() < 1e-3, "增量始终相对原始点列：{p:?}");
    assert_eq!(p.1, 61.0);
}

#[test]
fn test_drag_clamps_negative_tick_and_high_key() {
    let mut state = BrushToolState::default();
    state.begin_stroke((100.0, 250.0), 1);
    state.finish_stroke();
    state.begin_drag(0, (100.0, 250.0));
    state.drag_to((-500.0, 400.0));
    let p = state.strokes[0].points[0];
    assert_eq!(p.0, 0.0, "tick 不得为负");
    assert_eq!(p.1, 255.0, "key 上限 255");
}

#[test]
fn test_drag_unknown_stroke_rejected() {
    let mut state = BrushToolState::default();
    assert!(!state.begin_drag(7, (0.0, 0.0)), "越界笔画不可拖动");
    assert!(!state.is_active());
}

#[test]
fn test_stroke_bounds() {
    let stroke = BrushStroke {
        points: vec![(240.0, 60.0), (0.0, 67.0), (120.0, 58.0)],
        base_track: 3,
    };
    assert_eq!(stroke.bounds(), Some((0.0, 240.0, 58.0, 67.0)));
    assert_eq!(stroke.base_track, 3, "落笔基准轨随笔画记录");
    assert_eq!(BrushStroke::default().bounds(), None);
    assert!(BrushStroke::new((1.0, 2.0), 1).is_single_point());
    assert_eq!(BrushStroke::new((1.0, 2.0), 1).last(), Some((1.0, 2.0)));
}

// ── 笔画历史（独立于 document 历史） ─────────────────────────

#[test]
fn test_history_snapshot_undo_redo() {
    let mut state = BrushToolState::default();
    assert!(!state.can_undo_path() && !state.can_redo_path());

    state.begin_stroke((0.0, 60.0), 1);
    state.push_point((240.0, 60.0));
    state.finish_stroke();
    state.push_path_history();

    state.begin_stroke((960.0, 70.0), 1);
    state.finish_stroke();
    state.push_path_history();
    assert_eq!(state.strokes.len(), 2);

    assert!(state.undo_path());
    assert_eq!(state.strokes.len(), 1, "撤销一步回到第一笔");
    assert!(state.undo_path());
    assert!(state.strokes.is_empty(), "撤销两步回到空");
    assert!(!state.undo_path(), "无更多可撤销");

    assert!(state.can_redo_path());
    assert!(state.redo_path());
    assert_eq!(state.strokes.len(), 1);
    assert!(state.redo_path());
    assert_eq!(state.strokes.len(), 2);
    assert!(!state.redo_path());
}

#[test]
fn test_history_update_top_merges_continuous_drag() {
    let mut state = BrushToolState::default();
    state.begin_stroke((0.0, 60.0), 1);
    state.finish_stroke();
    state.push_path_history();

    // 拖动过程中只更新栈顶（合并为一次撤销）
    state.begin_drag(0, (0.0, 60.0));
    state.drag_to((240.0, 62.0));
    state.update_top_path_history();
    state.drag_to((480.0, 63.0));
    state.update_top_path_history();
    state.end_drag();
    assert_eq!(state.strokes[0].points[0], (480.0, 63.0));

    assert!(state.undo_path());
    assert!(state.strokes.is_empty(), "整段拖动只占一步撤销");
}

#[test]
fn test_history_truncates_redo_branch() {
    let mut state = BrushToolState::default();
    state.begin_stroke((0.0, 60.0), 1);
    state.finish_stroke();
    state.push_path_history();
    state.begin_stroke((480.0, 64.0), 1);
    state.finish_stroke();
    state.push_path_history();

    assert!(state.undo_path());
    assert!(state.can_redo_path());
    // 撤销后产生新操作 → 重做分支被截断
    state.begin_stroke((960.0, 68.0), 1);
    state.finish_stroke();
    state.push_path_history();
    assert!(!state.can_redo_path());
    assert_eq!(state.strokes.len(), 2);
}

#[test]
fn test_clear_pending_discards_strokes_and_history() {
    let mut state = BrushToolState::default();
    state.begin_stroke((0.0, 60.0), 1);
    state.finish_stroke();
    state.push_path_history();
    state.clear_pending();

    assert!(state.strokes.is_empty());
    assert!(!state.has_pending());
    assert!(
        !state.can_undo_path() && !state.can_redo_path(),
        "历史一并清空"
    );
    assert_eq!(state.path_history.len(), 1, "保留空基准状态");
}

#[test]
fn test_reset_clears_everything() {
    let mut state = BrushToolState::default();
    state.begin_stroke((0.0, 60.0), 1);
    state.reset();
    assert_eq!(state, BrushToolState::default());
}

// ── EditorState 集成（切工具 = ×） ─────────────────────────

#[test]
fn test_switch_tool_discards_pending_strokes() {
    let mut state = EditorState::new();
    state.tool = Tool::Brush;
    state.brush_tool.begin_stroke((0.0, 60.0), 1);
    state.brush_tool.finish_stroke();

    // 切到其他工具 = 丢弃（与曲线/形状/文字工具同处清理）
    state.set_tool(Tool::Pencil);
    assert!(
        !state.brush_tool.has_pending(),
        "切走工具必须丢弃待确认笔画（视为 ×）"
    );
}

#[test]
fn test_reselect_brush_keeps_pending_strokes() {
    let mut state = EditorState::new();
    state.tool = Tool::Brush;
    state.brush_tool.begin_stroke((0.0, 60.0), 1);
    state.brush_tool.finish_stroke();

    state.set_tool(Tool::Brush);
    assert!(state.brush_tool.has_pending(), "重新选中画刷不得清空笔画");
}

#[test]
fn test_editor_state_reset_clears_brush() {
    let mut state = EditorState::new();
    state.brush_tool.begin_stroke((0.0, 60.0), 1);
    state.reset();
    assert!(!state.brush_tool.has_pending(), "工程重置清空笔画状态");
}
