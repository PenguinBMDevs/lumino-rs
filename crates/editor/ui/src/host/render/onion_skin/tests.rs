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

fn assert_view_state(action: &OnionSkinAction) {
    assert!(
        matches!(action, OnionSkinAction::ViewState),
        "期望 ViewState，实际 {action:?}"
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
fn onion_skin_state_view_state_on_mute_change() {
    // 统一全量渲染：静音变化只更新 ViewState uniform（静音轨数据常驻，shader 掩码），零重传
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0b0000, 0, 0));
    assert_view_state(&state.decide_action(&make_fp(42, 0b0001, 0, 0)));
}

#[test]
fn onion_skin_state_view_state_on_track_switch() {
    // 统一全量渲染：切轨只更新 ViewState uniform（当前音轨段常驻，shader 着色），零重传
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 1, 0));
    assert_view_state(&state.decide_action(&make_fp(42, 0, 2, 0)));
}

#[test]
fn onion_skin_state_full_on_palette_switch() {
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 0, 1));
    assert_full(&state.decide_action(&make_fp(42, 0, 0, 2)));
}

// ── 增量豁免测试（编辑主音轨不再全量重建上传） ──────────────────────────

#[test]
fn onion_skin_state_delta_when_current_track_dirty() {
    // 统一全量渲染：当前音轨也在 GPU buffer 中，编辑当前音轨 → 段级替换该轨
    //（不是全量重建，也不是无操作）。
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 1, 0));
    let fp = make_fp_dirty(43, 1, std::collections::HashSet::from([1]), vec![]);
    assert_delta(&state.decide_action(&fp), &[1]);
}

#[test]
fn onion_skin_state_delta_consecutive_edits_same_track() {
    // 连续编辑当前音轨（非等长操作）→ 每次都是段级增量替换该轨，不累积全量
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 1, 0));
    for g in 43..48 {
        let fp = make_fp_dirty(g, 1, std::collections::HashSet::from([1]), vec![]);
        assert_delta(&state.decide_action(&fp), &[1]);
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
fn onion_skin_state_delta_includes_current_and_onion_tracks() {
    // 脏集合混合：当前音轨(1) + 静音音轨(4) + 洋葱皮音轨(3, 7)
    // → Delta 含当前音轨(1) + 洋葱皮音轨(3, 7)，静音音轨(4) 豁免
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 1, 0));
    let fp = make_fp_dirty(
        43,
        1,
        std::collections::HashSet::from([1, 3, 4, 7]),
        vec![4],
    );
    assert_delta(&state.decide_action(&fp), &[1, 3, 7]);
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
fn onion_skin_state_view_state_on_track_switch_after_current_delta() {
    // 当前音轨段级增量后切换当前音轨 → 只发 ViewState 零重传
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 1, 0));
    // 当前音轨 1 发生非等长编辑 → Delta([1]) 同步该轨
    let fp_skip = make_fp_dirty(43, 1, std::collections::HashSet::from([1]), vec![]);
    assert_delta(&state.decide_action(&fp_skip), &[1]);
    // 切换到音轨2 → ViewState（全量 buffer 常驻所有轨，切轨零重传）
    let fp_switch = make_fp_dirty(43, 2, std::collections::HashSet::from([1]), vec![]);
    assert_view_state(&state.decide_action(&fp_switch));
}

#[test]
fn onion_skin_state_view_state_on_track_switch_after_delta() {
    // Delta 后切轨 → ViewState（段表数据保持，仅显示语义变化）
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 1, 0));
    let fp = make_fp_dirty(43, 1, std::collections::HashSet::from([3]), vec![]);
    assert_delta(&state.decide_action(&fp), &[3]);
    let fp_switch = make_fp_dirty(43, 2, std::collections::HashSet::from([3]), vec![]);
    assert_view_state(&state.decide_action(&fp_switch));
}

#[test]
fn onion_skin_state_view_state_when_mute_changes_after_skipped_dirty() {
    // 豁免 gen 变更后 mute 变化 → ViewState（静音轨数据常驻，仅更新掩码）
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 1, 0));
    let fp = make_fp_dirty(43, 1, std::collections::HashSet::from([1]), vec![]);
    let mut rebuilt = fp;
    rebuilt.mute_fp = 999; // 模拟 mute 状态变化
    assert_view_state(&state.decide_action(&rebuilt));
}

#[test]
fn onion_skin_state_view_state_when_mute_changes_after_delta() {
    // Delta 后 mute 变化 → ViewState（零重传）
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 1, 0));
    let fp = make_fp_dirty(43, 1, std::collections::HashSet::from([3]), vec![]);
    assert_delta(&state.decide_action(&fp), &[3]);
    let mut rebuilt = fp;
    rebuilt.mute_fp = 999;
    assert_view_state(&state.decide_action(&rebuilt));
}

#[test]
fn mute_fingerprint_is_order_independent() {
    // 音轨拖拽排序只改变 sidebar.tracks 顺序，不改变静音集合。
    // 指纹必须顺序无关，否则排序会触发洋葱皮全量重建（不必要的 GPU 开销）。
    let fp1 = mute_fingerprint_of(&mut [3, 1, 5]);
    let fp2 = mute_fingerprint_of(&mut [1, 5, 3]);
    let fp3 = mute_fingerprint_of(&mut [3, 1, 5]);
    assert_eq!(fp1, fp2, "同一集合不同排列应产生相同指纹");
    assert_eq!(fp1, fp3);
}

#[test]
fn mute_fingerprint_distinguishes_sets() {
    let fp_empty = mute_fingerprint_of(&mut [] as &mut [usize]);
    let fp_one = mute_fingerprint_of(&mut [0]);
    let fp_two = mute_fingerprint_of(&mut [0, 1]);
    assert_ne!(fp_empty, fp_one);
    assert_ne!(fp_one, fp_two);
}
