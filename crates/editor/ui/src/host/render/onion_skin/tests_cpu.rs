//! CPU 单轨模式（Q3 B）洋葱皮决策测试。
//!
//! 语义：GPU 洋葱皮 buffer 只驻留当前轨（非当前轨/静音轨由 CPU 叠加层负责），
//! 因此只有「段内容 / 段身份」变化才重传，且**永不**产出 `Delta` / `ViewState`。
//! 独立文件以保持 `tests.rs` < 400 行；构造器与断言复用 `tests` 模块。

use super::tests::{assert_full, assert_none, make_fp, make_fp_dirty, make_fp_main_struct};
use super::*;

/// 构造一个「已在 CPU 单轨模式构造完成」的状态。
///
/// `set_cpu_current_only` 为了强制重建会置 `force_full`，`mark_built` 消费掉它——
/// 这样后续断言才是在测「指纹比较」而不是「模式切换」。
fn cpu_state(fp: &OnionSkinFingerprint) -> OnionSkinState {
    let mut state = OnionSkinState::default();
    state.set_cpu_current_only(true);
    state.mark_built(fp);
    state
}

#[test]
fn cpu_mode_first_run_is_full() {
    let mut state = OnionSkinState::default();
    state.set_cpu_current_only(true);
    assert_full(&state.decide_action(&make_fp(0, 0, 0, 0)));
}

#[test]
fn cpu_mode_track_switch_is_full() {
    let state = cpu_state(&make_fp(42, 0, 1, 0));
    assert_full(&state.decide_action(&make_fp(42, 0, 2, 0)));
}

#[test]
fn cpu_mode_track_count_change_is_full() {
    let state = cpu_state(&make_fp(42, 0, 1, 0));
    let mut fp = make_fp(42, 0, 1, 0);
    fp.track_count = 65;
    assert_full(&state.decide_action(&fp));
}

#[test]
fn cpu_mode_current_track_normal_edit_is_none() {
    // 当前轨普通编辑：段内 NoteEvent 增量由主轨事件通道负责，GPU 洋葱皮不动
    let state = cpu_state(&make_fp(42, 0, 1, 0));
    let fp = make_fp_dirty(43, 1, std::collections::HashSet::from([1]), vec![]);
    assert_none(&state.decide_action(&fp));
}

#[test]
fn cpu_mode_onion_track_edit_is_none() {
    // 洋葱皮轨（非当前/非静音）编辑：GPU 无该轨数据，重传没有意义
    let state = cpu_state(&make_fp(42, 0, 1, 0));
    let fp = make_fp_dirty(43, 1, std::collections::HashSet::from([3, 7]), vec![]);
    assert_none(&state.decide_action(&fp));
}

#[test]
fn cpu_mode_muted_track_edit_is_none() {
    let state = cpu_state(&make_fp(42, 0, 1, 0));
    let fp = make_fp_dirty(43, 1, std::collections::HashSet::from([4]), vec![4]);
    assert_none(&state.decide_action(&fp));
}

#[test]
fn cpu_mode_mute_and_palette_changes_are_none() {
    // 静音：GPU 只有当前轨，静音集合只影响洋葱轨显示语义 → 零重传
    // 调色板：当前轨颜色由 shader 主轨蓝强制覆盖，实例固化色无效 → 零重传
    let state = cpu_state(&make_fp(42, 0, 1, 1));
    assert_none(&state.decide_action(&make_fp(42, 999, 1, 1)));
    assert_none(&state.decide_action(&make_fp(42, 0, 1, 2)));
    let mut fp = make_fp(42, 0, 1, 1);
    fp.muted_tracks = vec![0, 4];
    assert_none(&state.decide_action(&fp));
}

#[test]
fn cpu_mode_current_track_struct_dirty_is_full() {
    // 当前轨结构性变化（批量插入/删除、undo 整轨替换）→ 单轨段重建
    let state = cpu_state(&make_fp(42, 0, 1, 0));
    assert_full(&state.decide_action(&make_fp_main_struct(1)));
}

#[test]
fn cpu_mode_unknown_source_is_full() {
    // 未知来源（undo/redo/加载/散改）→ 保守全量（与 GPU 模式同口径）
    let state = cpu_state(&make_fp(42, 0, 1, 0));
    assert_full(&state.decide_action(&make_fp(43, 0, 1, 0)));
}

#[test]
fn cpu_mode_never_emits_delta_or_view_state() {
    // 扫一遍各类指纹：CPU 模式不得产出 Delta（无多段）或 ViewState（无显示语义变化）
    let state = cpu_state(&make_fp(42, 0, 1, 1));
    let dirty_sets = [
        None,
        Some(std::collections::HashSet::new()),
        Some(std::collections::HashSet::from([1])),
        Some(std::collections::HashSet::from([3])),
        Some(std::collections::HashSet::from([1, 3, 4])),
    ];
    for track_gen in [42u64, 43] {
        for current in [1usize, 2] {
            for palette in [1u8, 9] {
                for mute in [0u64, 555] {
                    for track_count in [64usize, 70] {
                        for dirty in &dirty_sets {
                            for struct_dirty in [false, true] {
                                let fp = OnionSkinFingerprint {
                                    track_gen,
                                    mute_fp: mute,
                                    current_track: current,
                                    palette_idx: palette,
                                    onion_dirty_tracks: dirty.clone(),
                                    muted_tracks: vec![4],
                                    main_track_struct_dirty: struct_dirty,
                                    track_count,
                                };
                                let action = state.decide_action(&fp);
                                assert!(
                                    !matches!(
                                        action,
                                        OnionSkinAction::Delta(_) | OnionSkinAction::ViewState
                                    ),
                                    "CPU 单轨模式不得产出 Delta/ViewState，实际 {action:?}（gen={track_gen} current={current} palette={palette} mute={mute} track_count={track_count} dirty={dirty:?} struct={struct_dirty}）"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn cpu_mode_toggle_forces_next_full_once() {
    // 模式切换必须强制一次全量重建（段表语义从全轨变单轨），且只强制一次
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 1, 0));
    assert_none(&state.decide_action(&make_fp(42, 0, 1, 0)));

    state.set_cpu_current_only(true);
    assert_full(&state.decide_action(&make_fp(42, 0, 1, 0)));
    state.mark_built(&make_fp(42, 0, 1, 0));
    assert_none(&state.decide_action(&make_fp(42, 0, 1, 0)));

    // 重复设置同一取值不得再触发重建（每帧同步调用）
    state.set_cpu_current_only(true);
    assert_none(&state.decide_action(&make_fp(42, 0, 1, 0)));

    // 切回 GPU 全轨模式同样强制一次
    state.set_cpu_current_only(false);
    assert_full(&state.decide_action(&make_fp(42, 0, 1, 0)));
    state.mark_built(&make_fp(42, 0, 1, 0));
    assert!(matches!(
        state.decide_action(&make_fp_dirty(
            43,
            1,
            std::collections::HashSet::from([3]),
            vec![]
        )),
        OnionSkinAction::Delta(_)
    ));
}

#[test]
fn cpu_mode_flag_is_readable() {
    let mut state = OnionSkinState::default();
    assert!(!state.cpu_current_only, "默认必须是 GPU 全轨模式");
    state.set_cpu_current_only(true);
    assert!(state.cpu_current_only);
}
