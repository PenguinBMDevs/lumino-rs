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

// ── 生成音符长度下限（修改接口：x 分音符 → tick）────────────────────────

#[test]
fn test_min_note_length_ticks_follows_division() {
    // 128 分音符 = 全音符/128 = 4·ppq/128 = ppq/32
    assert_eq!(min_note_length_ticks(Some(128), 1920), 60.0);
    // 六十四分音符 = ppq/16、三十二分音符 = ppq/8
    assert_eq!(min_note_length_ticks(Some(64), 1920), 120.0);
    assert_eq!(min_note_length_ticks(Some(32), 1920), 240.0);
    // 非 2 的幂档位取整到整 tick（与填充切分同口径），且恒 ≥ 1 tick
    assert_eq!(min_note_length_ticks(Some(3), 480), 640.0);
    assert_eq!(min_note_length_ticks(Some(4096), 480), 1.0);
    // None = 不设下限（保持 RawNote 自带的 1 tick）
    assert_eq!(min_note_length_ticks(None, 1920), 1.0);
}

#[test]
fn test_to_note_triple_clamps_start_and_length() {
    // 起负 tick 钳到 0，长度补齐到下限（起点不动、只外扩终点）
    let short = RawNote {
        start: -5,
        end: -4,
        key: 60,
    };
    assert_eq!(to_note_triple(&short, 60.0), (0.0, 60, 60.0));

    // 长度已达标的音符逐位不变
    let long = RawNote {
        start: 1920,
        end: 3840,
        key: 64,
    };
    assert_eq!(to_note_triple(&long, 60.0), (1920.0, 64, 1920.0));

    // 竖直段（1 tick）补齐到下限
    let vertical = RawNote {
        start: 1920,
        end: 1921,
        key: 62,
    };
    assert_eq!(to_note_triple(&vertical, 60.0), (1920.0, 62, 60.0));
}

// ─── 全局绘制粗细的纵向铺层（expand_layers） ─────────────────────────────

/// 粗细 1（默认）= 原样返回：一条线仍然是「每条音高行一条音符」，零行为变化。
#[test]
fn test_expand_layers_thickness_one_is_identity() {
    let base = sorted(path_notes(&[(0.0, 60.0), (3840.0, 62.0)], false));
    assert_eq!(base.len(), 3, "前置：key 60..62 共 3 条音高行音符");

    let expanded = expand_layers(base.clone(), 1);
    assert_eq!(expanded, base, "粗细 1 不得改变任何音符");
    // 0 也视作"不铺层"（防御性：上游任何路径都不该写出 0）
    assert_eq!(expand_layers(base.clone(), 0), base);
}

/// 粗细 N = 每条音符向上铺 N 层（key + level），tick / 长度逐位不变。
///
/// 层序与画刷笔画同口径：level 0 = 原行，向上逐层——这是"画出来多粗"
/// （线宽 = 层数 × 单 key 高）与"生成多高"同源的几何依据。
#[test]
fn test_expand_layers_stacks_upward_and_keeps_timing() {
    let base = vec![RawNote {
        start: 0,
        end: 960,
        key: 60,
    }];

    let expanded = sorted(expand_layers(base, 3));
    assert_eq!(
        expanded,
        vec![
            RawNote {
                start: 0,
                end: 960,
                key: 60
            },
            RawNote {
                start: 0,
                end: 960,
                key: 61
            },
            RawNote {
                start: 0,
                end: 960,
                key: 62
            },
        ],
        "3 层应得到 key 60/61/62 三条同 tick 同长度音符"
    );
}

/// 铺层不裁剪越界 key：越界层由调用方既有的 `key < key_count` 过滤裁掉
/// （展开与裁剪分两处，是为了让曲线 / 形状两条链路共用同一份钳位口径）。
#[test]
fn test_expand_layers_leaves_out_of_range_keys_for_caller() {
    let base = vec![RawNote {
        start: 0,
        end: 10,
        key: 126,
    }];
    let expanded = sorted(expand_layers(base, 5));
    assert_eq!(expanded.len(), 5);
    assert_eq!(
        expanded.iter().map(|n| n.key).collect::<Vec<_>>(),
        vec![126, 127, 128, 129, 130],
        "展开层原样保留越界 key（含 >127），由调用方的 key_count 过滤裁剪"
    );
}

/// 多条音符的展开是**按音符独立**的（互不串行、层数不累加）。
#[test]
fn test_expand_layers_multiplies_note_count() {
    let base = sorted(path_notes(&[(0.0, 60.0), (3840.0, 64.0)], false));
    let n = base.len();
    let expanded = expand_layers(base, 4);
    assert_eq!(
        expanded.len(),
        n * 4,
        "每条音符独立铺 4 层：{n} 条应得到 {} 条",
        n * 4
    );
}
