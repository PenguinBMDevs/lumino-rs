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
        main_track_struct_dirty: false,
        track_count: 64,
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
        main_track_struct_dirty: false,
        track_count: 64,
    }
}

/// 构造带主轨结构性脏标记的指纹（gen 未变，仅有结构性变化）
fn make_fp_main_struct(current_track: usize) -> OnionSkinFingerprint {
    OnionSkinFingerprint {
        track_gen: 42,
        mute_fp: 0,
        current_track,
        palette_idx: 0,
        onion_dirty_tracks: Some(std::collections::HashSet::from([current_track])),
        muted_tracks: Vec::new(),
        main_track_struct_dirty: true,
        // 当前轨存在于文档（守卫通过）；越界用例单独覆写
        track_count: current_track + 1,
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
    assert_eq!(state.last_track_count, 0);
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
fn onion_skin_state_none_when_current_track_dirty() {
    // 统一全量渲染：当前音轨由主音轨事件级增量同步，
    // 不再走洋葱皮的 TrackDelta。
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 1, 0));
    let fp = make_fp_dirty(43, 1, std::collections::HashSet::from([1]), vec![]);
    assert_none(&state.decide_action(&fp));
}

#[test]
fn onion_skin_state_none_consecutive_edits_same_current_track() {
    // 连续编辑当前音轨 → 由主音轨事件级增量同步，洋葱皮无操作
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
fn onion_skin_state_delta_includes_onion_tracks_not_current() {
    // 脏集合混合：当前音轨(1) + 静音音轨(4) + 洋葱皮音轨(3, 7)
    // → Delta 只含洋葱皮音轨(3, 7)，当前音轨由事件增量同步，静音音轨(4) 豁免
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
fn onion_skin_state_delta_main_track_struct_change() {
    // 主轨结构性变化（大插入/undo 整轨替换，gen 未变）→ 当前轨加入 Delta（单轨段重建），
    // 不再走全量会话重建
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 1, 0));
    let fp = make_fp_main_struct(1);
    assert_delta(&state.decide_action(&fp), &[1]);
}

#[test]
fn onion_skin_state_delta_main_track_struct_with_onion_dirty() {
    // 主轨结构性变化 + 其他洋葱皮音轨同时脏 → Delta 含当前轨与洋葱皮音轨
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 1, 0));
    let mut fp = make_fp_main_struct(1);
    fp.track_gen = 43;
    fp.onion_dirty_tracks = Some(std::collections::HashSet::from([1, 3]));
    assert_delta(&state.decide_action(&fp), &[1, 3]);
}

#[test]
fn onion_skin_state_main_struct_current_beyond_track_count_is_ignored() {
    // 守卫：当前轨越界（文档轨数不足）→ 不产生 Delta（无段可重建）。
    // 典型场景：侧边栏新增轨（doc 扩轨）但 TrackLayout 尚未同步。
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 15, 0));
    let mut fp = make_fp_main_struct(15);
    fp.track_count = 15; // 文档只有 0..14
    assert_none(&state.decide_action(&fp));
}

#[test]
fn onion_skin_state_track_count_change_alone_is_none() {
    // 轨数变化本身不改变 decide_action 输出（布局同步由调用方独立执行
    // `TrackLayout`），避免误触发全量重建
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 1, 0));
    let mut fp = make_fp(42, 0, 1, 0);
    fp.track_count = 65;
    assert_none(&state.decide_action(&fp));
}

#[test]
fn onion_skin_state_mark_built_records_track_count() {
    let mut state = OnionSkinState::default();
    assert_eq!(state.last_track_count(), 0);
    let mut fp = make_fp(42, 0, 1, 0);
    fp.track_count = 16;
    state.mark_built(&fp);
    assert_eq!(state.last_track_count(), 16);
}

#[test]
fn onion_skin_state_view_state_defers_main_track_struct_change() {
    // 布局变化帧返回 ViewState（数据零重传）；下一帧同指纹仍应产出 Delta(当前轨)，
    // 由渲染层保留 main_track_struct_dirty 保证不丢失主轨段重建
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 1, 0));
    // 模拟切轨帧（mute_fp 变化）
    let mut layout_fp = make_fp_main_struct(1);
    layout_fp.mute_fp = 7;
    assert_view_state(&state.decide_action(&layout_fp));
    state.mark_built(&layout_fp);
    // 下一帧布局稳定（mute 指纹保持 7，主轨结构标记仍在）→ Delta(当前轨)
    let mut stable_fp = make_fp_main_struct(1);
    stable_fp.mute_fp = 7;
    assert_delta(&state.decide_action(&stable_fp), &[1]);
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
    // 当前音轨事件级增量 → 洋葱皮无操作；切换当前音轨 → 只发 ViewState 零重传
    let mut state = OnionSkinState::default();
    state.mark_built(&make_fp(42, 0, 1, 0));
    // 当前音轨 1 发生非等长编辑 → 主音轨事件增量同步；洋葱皮不重建
    let fp_skip = make_fp_dirty(43, 1, std::collections::HashSet::from([1]), vec![]);
    assert_none(&state.decide_action(&fp_skip));
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
