//! 路径 → 音符（蜘蛛网式逐音高行跨越）测试
//!
//! 断言的是**几何语义**而非格点数量：一条音高行一条音符、无缝连奏、
//! 长度由曲线与行边界的解析交点决定，与任何"设定精度"无关。

use super::*;

/// 音符按 (start, key) 排序，便于断言集合而不受内部生成顺序影响
fn sorted(mut notes: Vec<RawNote>) -> Vec<RawNote> {
    notes.sort_by_key(|n| (n.start, n.key, n.end));
    notes
}

#[test]
fn test_horizontal_line_is_one_full_length_note() {
    // 水平线只经过一条音高行 → 恰好一条音符铺满整条线（不是 N 个定长格子）
    let notes = path_notes(&[(0.0, 60.0), (3840.0, 60.0)], false);
    assert_eq!(
        notes,
        vec![RawNote {
            start: 0,
            end: 3840,
            key: 60
        }]
    );
}

#[test]
fn test_diagonal_line_gives_even_rows_seamless() {
    // key 60 → 64 的直线：5 条音高行各分到等长的一份时间（spiderweb 签名行为）
    let notes = sorted(path_notes(&[(0.0, 60.0), (3840.0, 64.0)], false));
    assert_eq!(notes.len(), 5, "5 条音高行 → 5 条音符");
    assert_eq!(
        notes,
        vec![
            RawNote {
                start: 0,
                end: 768,
                key: 60
            },
            RawNote {
                start: 768,
                end: 1536,
                key: 61
            },
            RawNote {
                start: 1536,
                end: 2304,
                key: 62
            },
            RawNote {
                start: 2304,
                end: 3072,
                key: 63
            },
            RawNote {
                start: 3072,
                end: 3840,
                key: 64
            },
        ],
        "首尾各占满一整份、两两无缝衔接、末音符尾部对齐锚点"
    );
}

#[test]
fn test_diagonal_has_no_gap_and_no_overflow() {
    // 无缝 + 不越界：总覆盖 = 曲线跨度，任意相邻音符首尾相接
    let notes = sorted(path_notes(&[(0.0, 60.0), (3840.0, 64.0)], false));
    assert_eq!(notes.first().map(|n| n.start), Some(0));
    assert_eq!(notes.last().map(|n| n.end), Some(3840));
    for w in notes.windows(2) {
        assert_eq!(w[0].end, w[1].start, "音符之间不得有空隙");
    }
}

#[test]
fn test_vertical_line_is_one_tick_per_row() {
    // 竖直段：所有音高行落在同一个 tick → 除最后一条外都只占 1 tick
    let notes = sorted(path_notes(&[(1920.0, 60.0), (1920.0, 64.0)], false));
    assert_eq!(notes.len(), 5);
    assert!(notes.iter().all(|n| n.start == 1920), "全部起于同一 tick");
    assert!(
        notes.iter().all(|n| n.length() == 1),
        "陡峭段不得把长音符叠成实心块: {notes:?}"
    );
}

#[test]
fn test_right_to_left_path_same_as_left_to_right() {
    // 从右往左画的同一条线必须得到同一组音符
    let a = sorted(path_notes(&[(0.0, 60.0), (3840.0, 64.0)], false));
    let b = sorted(path_notes(&[(3840.0, 64.0), (0.0, 60.0)], false));
    assert_eq!(a, b);
}

#[test]
fn test_closed_loop_has_no_end_stretch() {
    // 闭合矩形：顶边 / 底边各铺满整宽；两条竖直边只是"瞬间穿过"每一行 →
    // 1 tick（`ends` 下限 1 tick）；闭合环不做首尾拉伸。
    let path = [
        (0.0, 60.0),
        (960.0, 60.0),
        (960.0, 62.0),
        (0.0, 62.0),
        (0.0, 60.0),
    ];
    assert_eq!(
        sorted(path_notes(&path, false)),
        vec![
            RawNote {
                start: 0,
                end: 960,
                key: 60
            },
            RawNote {
                start: 0,
                end: 1,
                key: 61
            },
            RawNote {
                start: 0,
                end: 960,
                key: 62
            },
            RawNote {
                start: 960,
                end: 961,
                key: 60
            },
            RawNote {
                start: 960,
                end: 961,
                key: 61
            },
            RawNote {
                start: 960,
                end: 961,
                key: 62
            },
        ],
        "顶/底边铺满，竖直边逐行 1 tick"
    );
}

#[test]
fn test_single_point_path_makes_one_tick() {
    let notes = path_notes(&[(100.0, 60.0)], false);
    assert_eq!(
        notes,
        vec![RawNote {
            start: 100,
            end: 101,
            key: 60
        }]
    );
}

#[test]
fn test_duplicate_points_collapse() {
    // 重复点不影响结果
    let a = path_notes(&[(0.0, 60.0), (0.0, 60.0), (3840.0, 64.0)], false);
    let b = path_notes(&[(0.0, 60.0), (3840.0, 64.0)], false);
    assert_eq!(sorted(a), sorted(b));
}

#[test]
fn test_end_dot_moves_last_note_to_the_endpoint() {
    // end_dot：末音符**起于**末点（而不是终于末点），门长与前一条相同
    let plain = sorted(path_notes(&[(0.0, 60.0), (3840.0, 63.0)], false));
    let dotted = sorted(path_notes(&[(0.0, 60.0), (3840.0, 63.0)], true));
    assert_eq!(
        plain.last().map(|n| n.end),
        Some(3840),
        "默认：尾部对齐末点"
    );
    let last = *dotted.last().expect("应有音符");
    assert_eq!(last.start, 3840, "end_dot：末音符起于末点");
    let prev = dotted[dotted.len() - 2];
    assert_eq!(
        last.length(),
        prev.length(),
        "end_dot 末音符取与前一条相同的门长"
    );
}

#[test]
fn test_keep_longest_wins_by_length() {
    let notes = keep_longest(&[
        RawNote {
            start: 0,
            end: 100,
            key: 60,
        },
        RawNote {
            start: 0,
            end: 300,
            key: 60,
        },
        RawNote {
            start: 0,
            end: 100,
            key: 61,
        },
        RawNote {
            start: 50,
            end: 60,
            key: 60,
        },
    ]);
    assert_eq!(
        notes,
        vec![
            RawNote {
                start: 0,
                end: 300,
                key: 60
            },
            RawNote {
                start: 0,
                end: 100,
                key: 61
            },
            RawNote {
                start: 50,
                end: 60,
                key: 60
            },
        ],
        "同 tick 同 key 只留最长的一条，顺序按各组首次出现"
    );
}

#[test]
fn test_keep_longest_drops_notes_ending_before_zero() {
    let notes = keep_longest(&[
        RawNote {
            start: -10,
            end: -1,
            key: 60,
        },
        RawNote {
            start: 0,
            end: 10,
            key: 61,
        },
    ]);
    assert_eq!(
        notes,
        vec![RawNote {
            start: 0,
            end: 10,
            key: 61
        }]
    );
}

#[test]
fn test_direction_changes_ignores_still_segments() {
    // 原地不动的段跳过：只与"上一个真的动了的段"比较
    assert!(direction_changes(&[0.0, 1.0, 1.0, 2.0, 3.0]).is_empty());
    // 上 → 下 → 上：两处都算方向反转
    assert_eq!(direction_changes(&[0.0, 5.0, 3.0, 8.0]), vec![1, 2]);
}
