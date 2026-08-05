use super::*;

/// 辅助函数：构造测试用指纹
fn make_fp(
    track_gen: u64,
    mute_fp: u64,
    current_track: usize,
    palette_idx: u8,
) -> OnionSkinFingerprint {
    OnionSkinFingerprint {
        track_gen,
        mute_fp,
        current_track,
        palette_idx,
        onion_dirty_tracks: None,
        muted_tracks: Vec::new(),
    }
}

/// 构造带音轨级脏标记的指纹
fn make_fp_dirty(
    track_gen: u64,
    current_track: usize,
    dirty_tracks: std::collections::HashSet<usize>,
    muted_tracks: Vec<usize>,
) -> OnionSkinFingerprint {
    OnionSkinFingerprint {
        track_gen,
        mute_fp: 0,
        current_track,
        palette_idx: 0,
        onion_dirty_tracks: Some(dirty_tracks),
        muted_tracks,
    }
}

fn assert_none(action: &OnionSkinAction) {
    assert!(
        matches!(action, OnionSkinAction::None),
        "期望 None，实际 {action:?}"
    );
}

fn assert_full(action: &OnionSkinAction) {
    assert!(
        matches!(action, OnionSkinAction::Full),
        "期望 Full，实际 {action:?}"
    );
}

fn assert_delta(action: &OnionSkinAction, expected: &[usize]) {
    match action {
        OnionSkinAction::Delta(tracks) => {
            // Delta 音轨顺序来自 HashSet 迭代（无语义），按集合比较
            let mut actual = tracks.clone();
            let mut want = expected.to_vec();
            actual.sort_unstable();
            want.sort_unstable();
            assert_eq!(actual, want, "Delta 音轨集合不匹配");
        }
        other => panic!("期望 Delta({expected:?})，实际 {other:?}"),
    }
}

#[test]
fn onion_skin_state_default_uninitialized() {
    let state = OnionSkinState::default();
    assert!(!state.initialized);
    assert_eq!(state.last_track_notes_gen, 0);
    assert_eq!(state.last_mute_fingerprint, 0);
    assert_eq!(state.last_current_track, usize::MAX);
    assert_eq!(state.last_palette_idx, u8::MAX);
}

#[test]
fn onion_skin_state_full_on_first_run() {
    let state = OnionSkinState::default();
    assert_full(&state.decide_action(&make_fp(0, 0, 0, 0)));
}

#[test]
fn onion_skin_state_none_after_mark_built() {
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0b1010, 3, 1));
    assert_none(&state.decide_action(&make_fp(42, 0b1010, 3, 1)));
}

#[test]
fn onion_skin_state_full_on_gen_change_unknown() {
    // 无参 mark（脏音轨未知）→ 保守全量
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 0, 0));
    assert_full(&state.decide_action(&make_fp(43, 0, 0, 0)));
}

#[test]
fn onion_skin_state_full_on_mute_change() {
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0b0000, 0, 0));
    assert_full(&state.decide_action(&make_fp(42, 0b0001, 0, 0)));
}

#[test]
fn onion_skin_state_full_on_track_switch() {
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 1, 0));
    assert_full(&state.decide_action(&make_fp(42, 0, 2, 0)));
}

#[test]
fn onion_skin_state_full_on_palette_switch() {
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 0, 1));
    assert_full(&state.decide_action(&make_fp(42, 0, 0, 2)));
}

// ── 增量豁免测试（编辑主音轨不再全量重建上传） ──────────────────────────

#[test]
fn onion_skin_state_none_when_only_current_track_dirty() {
    // 编辑当前音轨 → 洋葱皮不显示该音轨 → 数据未变 → 豁免全量重建上传
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 1, 0));
    let fp = make_fp_dirty(43, 1, std::collections::HashSet::from([1]), vec![]);
    assert_none(&state.decide_action(&fp));
}

#[test]
fn onion_skin_state_none_consecutive_edits_same_track() {
    // 连续编辑当前音轨（拖动热路径每帧触发）应持续豁免，不累积重建
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 1, 0));
    for g in 43..48 {
        let fp = make_fp_dirty(g, 1, std::collections::HashSet::from([1]), vec![]);
        assert_none(&state.decide_action(&fp));
    }
}

#[test]
fn onion_skin_state_none_when_dirty_track_muted() {
    // 变化音轨是静音音轨 → 洋葱皮不显示 → 豁免
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 2, 0));
    let fp = make_fp_dirty(43, 2, std::collections::HashSet::from([0]), vec![0]);
    assert_none(&state.decide_action(&fp));
}

// ── 事件级增量测试（编辑洋葱皮音轨 → 段级替换，不全量重建） ────────────

#[test]
fn onion_skin_state_delta_when_other_track_dirty() {
    // 编辑了非当前音轨 → 洋葱皮显示它 → 事件级增量（只传该音轨）
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 1, 0));
    let fp = make_fp_dirty(43, 1, std::collections::HashSet::from([3]), vec![]);
    assert_delta(&state.decide_action(&fp), &[3]);
}

#[test]
fn onion_skin_state_delta_filters_to_onion_tracks_only() {
    // 脏集合混合：当前音轨(1) + 静音音轨(4) + 洋葱皮音轨(3, 7)
    // → Delta 只含洋葱皮音轨
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 1, 0));
    let fp = make_fp_dirty(
        43,
        1,
        std::collections::HashSet::from([1, 3, 4, 7]),
        vec![4],
    );
    assert_delta(&state.decide_action(&fp), &[3, 7]);
}

#[test]
fn onion_skin_state_delta_consecutive_edits_same_onion_track() {
    // 连续编辑同一洋葱皮音轨（拖动热路径）→ 每次都是段级增量，不累积全量
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 1, 0));
    for g in 43..48 {
        let fp = make_fp_dirty(g, 1, std::collections::HashSet::from([3]), vec![]);
        assert_delta(&state.decide_action(&fp), &[3]);
    }
}

#[test]
fn onion_skin_state_delta_multi_track_edits() {
    // 同时编辑两个洋葱皮音轨 → Delta 含两者
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 1, 0));
    let fp = make_fp_dirty(43, 1, std::collections::HashSet::from([2, 5]), vec![]);
    assert_delta(&state.decide_action(&fp), &[2, 5]);
}

#[test]
fn onion_skin_state_none_after_delta_mark_built() {
    // Delta 后 mark_built → 同 gen 不再重复构建
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 1, 0));
    let fp = make_fp_dirty(43, 1, std::collections::HashSet::from([3]), vec![]);
    assert_delta(&state.decide_action(&fp), &[3]);
    state.mark_built(&fp);
    assert_none(&state.decide_action(&fp));
}

#[test]
fn onion_skin_state_full_when_dirty_unknown() {
    // 变化来源未知（None）→ 保守全量重建
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 1, 0));
    assert_full(&state.decide_action(&make_fp(43, 0, 1, 0)));
}

#[test]
fn onion_skin_state_full_on_track_switch_after_skipped_dirty() {
    // 豁免后切换当前音轨 → 音轨切换本身必须触发全量（用最新数据兜底被豁免的编辑）
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 1, 0));
    // 豁免一次（编辑音轨1，当前也是1）
    let fp_skip = make_fp_dirty(43, 1, std::collections::HashSet::from([1]), vec![]);
    assert_none(&state.decide_action(&fp_skip));
    // 切换到音轨2 → 必须全量（旧被豁免的编辑此时成为洋葱皮数据）
    let fp_switch = make_fp_dirty(43, 2, std::collections::HashSet::from([1]), vec![]);
    assert_full(&state.decide_action(&fp_switch));
}

#[test]
fn onion_skin_state_full_on_track_switch_after_delta() {
    // Delta 后切换当前音轨 → 全量重建段表（段表布局变化，增量无法安全应用）
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 1, 0));
    let fp = make_fp_dirty(43, 1, std::collections::HashSet::from([3]), vec![]);
    assert_delta(&state.decide_action(&fp), &[3]);
    let fp_switch = make_fp_dirty(43, 2, std::collections::HashSet::from([3]), vec![]);
    assert_full(&state.decide_action(&fp_switch));
}

#[test]
fn onion_skin_state_full_when_mute_changes_after_skipped_dirty() {
    // 豁免 gen 变更后 mute 状态变化 → 仍须全量（段表布局变化）
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 1, 0));
    let fp = make_fp_dirty(43, 1, std::collections::HashSet::from([1]), vec![]);
    let mut rebuilt = fp;
    rebuilt.mute_fp = 999; // 模拟 mute 状态变化
    assert_full(&state.decide_action(&rebuilt));
}

#[test]
fn onion_skin_state_full_when_mute_changes_after_delta() {
    // Delta 后 mute 变化 → 全量（静音音轨进出洋葱皮，段表失效）
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 1, 0));
    let fp = make_fp_dirty(43, 1, std::collections::HashSet::from([3]), vec![]);
    assert_delta(&state.decide_action(&fp), &[3]);
    let mut rebuilt = fp;
    rebuilt.mute_fp = 999;
    assert_full(&state.decide_action(&rebuilt));
}
