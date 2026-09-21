
use super::types::MAX_SPAWNS_PER_KEY_PER_BLOCK;
use super::voice_alloc::select_release_note_id;
use super::{
    ChannelState, RenderCheckpoint, checkpoint_ok, limit_block, select_damper_release_groups,
    select_evictions, spawn_budget_allows,
};
use crate::error::SynthError;
use crate::synth::voices::test_voice;

const LOOKAHEAD: usize = 256;

#[test]
fn limiter_kills_single_sample_spike() {
    let n = 512usize;
    let mut tail = vec![0.0f32; LOOKAHEAD * 2];
    let mut gain = 1.0f32;
    let mut out = vec![0.5f32; n * 2];
    // Single-sample +3 spike at frame 100; the limiter delays by
    // LOOKAHEAD so it is emitted at output frame 356.
    out[100 * 2] = 3.0;
    out[100 * 2 + 1] = 3.0;

    limit_block(&mut out, &mut tail, &mut gain, 64000.0);

    // The original forward-only window missed this spike entirely; the new
    // window (centred on the emitted sample) must attenuate it. soft_knee
    // caps the output at 1.0, so a surviving spike would blow past that.
    let max_abs = out.iter().map(|x| x.abs()).fold(0.0f32, f32::max);
    assert!(
        max_abs < 1.0,
        "single-sample spike not limited (max abs = {max_abs})"
    );
}

#[test]
fn limiter_kills_end_of_block_spike_across_blocks() {
    let n = 512usize;
    let mut tail = vec![0.0f32; LOOKAHEAD * 2];
    let mut gain = 1.0f32;
    // Block 0 ends with a spike in its very last sample.
    let mut block0 = vec![0.5f32; n * 2];
    block0[(n - 1) * 2] = 3.0;
    block0[(n - 1) * 2 + 1] = 3.0;
    limit_block(&mut block0, &mut tail, &mut gain, 64000.0);

    // Block 1 is quiet; the spike now lives in 	ail and is emitted near
    // block 1's start, so the gain window must reach into the tail to see it.
    let mut block1 = vec![0.5f32; n * 2];
    limit_block(&mut block1, &mut tail, &mut gain, 64000.0);

    for (name, b) in [("block0", &block0), ("block1", &block1)] {
        let max_abs = b.iter().map(|x| x.abs()).fold(0.0f32, f32::max);
        assert!(
            max_abs < 1.0,
            "{name}: end-of-block spike not limited (max abs = {max_abs})"
        );
    }
}

#[test]
fn limiter_sanitizes_nonfinite() {
    let n = 512usize;
    let mut tail = vec![0.0f32; LOOKAHEAD * 2];
    let mut gain = 1.0f32;
    let mut out = vec![0.5f32; n * 2];
    out[50 * 2] = f32::NAN;
    out[50 * 2 + 1] = f32::INFINITY;

    limit_block(&mut out, &mut tail, &mut gain, 64000.0);

    assert!(out[50 * 2].is_finite(), "NaN was not sanitized");
    assert!(out[50 * 2 + 1].is_finite(), "Inf was not sanitized");
    let max_abs = out.iter().map(|x| x.abs()).fold(0.0f32, f32::max);
    assert!(
        max_abs < 1.0,
        "non-finite artifact leaked a pop (max abs = {max_abs})"
    );
}

/// 标准 CC 编码驱动 RPN 0（弯音灵敏度）：CC101=MSB、CC100=LSB、CC6/CC38=数据。
/// 旧实现把 CC100 当 MSB、CC101 当 LSB，标准编码的 RPN 1/2 永远无法生效。
#[test]
fn cc_driven_rpn_pitch_bend_sensitivity() {
    let mut st = ChannelState::new();
    assert!(!st.handle_rpn_cc(0x65, 0)); // CC101: RPN MSB = 0
    assert!(!st.handle_rpn_cc(0x64, 0)); // CC100: RPN LSB = 0
    assert!(st.handle_rpn_cc(0x06, 12)); // CC6: sensitivity = 12 半音
    assert!(
        (st.bend_sensitivity - 12.0).abs() < 1e-3,
        "sensitivity = {} (expected 12; CC100/CC101 wiring)",
        st.bend_sensitivity
    );
    // 12 半音灵敏度下，bend=9000（中心 +808）≈ +1.18 半音。
    st.bend_value = 9000;
    st.recompute_pitch();
    let expected = 2.0f32.powf(((9000.0 - 8192.0) / 8192.0 * 12.0) / 12.0);
    assert!((st.pitch_multiplier - expected).abs() < 1e-4);
    // LSB 为 1/128 半音（128 精度）：CC38=64 → +0.5 半音。
    assert!(st.handle_rpn_cc(0x26, 64));
    assert!(
        (st.bend_sensitivity - 12.5).abs() < 1e-3,
        "sensitivity = {} (expected 12.5, /128 precision)",
        st.bend_sensitivity
    );
    // 默认灵敏度仍为 2（GM）。
    let st2 = ChannelState::new();
    assert!((st2.bend_sensitivity - 2.0).abs() < 1e-3);
}

/// RPN 2 粗调必须能被标准编码驱动（旧接线下永远不生效）。
#[test]
fn cc_driven_rpn_coarse_tuning() {
    let mut st = ChannelState::new();
    st.handle_rpn_cc(0x65, 0); // MSB
    st.handle_rpn_cc(0x64, 2); // LSB = 2 → RPN 0:2
    assert!(st.handle_rpn_cc(0x06, 67)); // +3 半音
    st.recompute_pitch();
    let expected = 2.0f32.powf(3.0 / 12.0);
    assert!(
        (st.pitch_multiplier - expected).abs() < 1e-4,
        "coarse mult = {} (expected {})",
        st.pitch_multiplier,
        expected
    );
}

/// RPN 1 微调：标准 14-bit（中心 8192），LSB 先到也应正确。
#[test]
fn cc_driven_rpn_fine_tuning_lsb_first() {
    let mut st = ChannelState::new();
    st.handle_rpn_cc(0x64, 1); // LSB = 1（先发）
    st.handle_rpn_cc(0x65, 0); // MSB = 0
    st.handle_rpn_cc(0x06, 64); // 数据 MSB
    st.handle_rpn_cc(0x26, 0); // 数据 LSB → 中心
    assert!(st.fine_cents.abs() < 1e-3, "fine = {}", st.fine_cents);
    st.handle_rpn_cc(0x26, 64);
    let expected_cents = (64.0 / 8192.0) * 100.0;
    assert!(
        (st.fine_cents - expected_cents).abs() < 1e-3,
        "fine = {} (expected {})",
        st.fine_cents,
        expected_cents
    );
}

/// NRPN 选中期间的数据入口必须被消费丢弃（MIDI 规范：RPN/NRPN 独立命名空间），
/// 不能落进上一次 RPN 选择（否则会把 NRPN 的值写进弯音灵敏度）。
#[test]
fn nrpn_data_entry_is_consumed() {
    let mut st = ChannelState::new();
    st.handle_rpn_cc(0x65, 0);
    st.handle_rpn_cc(0x64, 0);
    st.handle_rpn_cc(0x06, 12);
    assert!((st.bend_sensitivity - 12.0).abs() < 1e-3);
    // 切到 NRPN（CC99=MSB / CC98=LSB），再发数据入口。
    st.handle_rpn_cc(0x63, 1);
    st.handle_rpn_cc(0x62, 8);
    st.handle_rpn_cc(0x06, 64);
    st.handle_rpn_cc(0x26, 127);
    assert!(
        (st.bend_sensitivity - 12.0).abs() < 1e-3,
        "NRPN data leaked into RPN 0: sensitivity = {}",
        st.bend_sensitivity
    );
    // 切回 RPN 0 后数据入口恢复生效。
    st.handle_rpn_cc(0x65, 0);
    st.handle_rpn_cc(0x64, 0);
    st.handle_rpn_cc(0x06, 2);
    assert!((st.bend_sensitivity - 2.0).abs() < 1e-3);
}

#[test]
fn render_checkpoint_cancel_semantics() {
    let none: Option<RenderCheckpoint> = None;
    assert!(checkpoint_ok(&none).is_ok());
    let run: Option<RenderCheckpoint> = Some(std::sync::Arc::new(|| true));
    assert!(checkpoint_ok(&run).is_ok());
    let cancel: Option<RenderCheckpoint> = Some(std::sync::Arc::new(|| false));
    assert!(matches!(checkpoint_ok(&cancel), Err(SynthError::Cancelled)));
}

#[test]
fn spawn_guard_only_caps_pathological_bursts() {
    // 常规密度（含黑 MIDI 单键单块千级 note-on）必须全部放行：
    // 旧实现拿 max_voices_per_key 当预算，丢的是最新音符（limit=4 时
    // 实测丢 18% note-on），已废弃。
    assert!(spawn_budget_allows(0));
    assert!(spawn_budget_allows(2_688));
    assert!(spawn_budget_allows(MAX_SPAWNS_PER_KEY_PER_BLOCK - 1));
    // 只有病态风暴（单键单块 6.5 万+ note-on）才触发保护上限。
    assert!(!spawn_budget_allows(MAX_SPAWNS_PER_KEY_PER_BLOCK));
    assert!(!spawn_budget_allows(u32::MAX));
}

#[test]
fn evictions_protect_newest_group_even_when_quietest() {
    // (spawn_frame, vel, note_id)：note_id=3 最新但力度最低。
    let groups = [(0, 100, 1), (1, 60, 2), (2, 5, 3)];
    let evict = select_evictions(&groups, 2, Some(3));
    // 保护 3，其余按 (vel, note_id) 升序：2(60) 先于 1(100)。
    assert_eq!(evict, vec![1, 0]);
}

#[test]
fn evictions_default_protection_is_max_note_id() {
    let groups = [(0, 100, 1), (1, 60, 2), (2, 5, 3)];
    let evict = select_evictions(&groups, 2, None);
    assert_eq!(evict, vec![1, 0]);
}

#[test]
fn evictions_prefer_quietest_then_oldest() {
    // 保护 note_id=4；候选按 (vel, note_id)：1(vel 5)、3(vel 60)、2(vel 100)。
    let groups = [(0, 5, 1), (1, 100, 2), (2, 60, 3), (3, 20, 4)];
    let evict = select_evictions(&groups, 2, Some(4));
    assert_eq!(evict, vec![0, 2]);
}

#[test]
fn evictions_limit_one_keeps_latest() {
    let groups = [(0, 127, 1), (1, 1, 2)];
    let evict = select_evictions(&groups, 1, Some(2));
    assert_eq!(evict, vec![0]);
}

#[test]
fn evictions_handle_empty_and_zero_request() {
    assert!(select_evictions(&[], 3, None).is_empty());
    let groups = [(0, 10, 1), (1, 20, 2)];
    assert!(select_evictions(&groups, 0, None).is_empty());
    assert_eq!(select_evictions(&groups, 1, Some(2)), vec![0]);
}

/// #42：踏板踩下期间已登记"待释放"的组，后续 NoteOff 必须跳过它，
/// 否则同一个组会被重复配对、把更新的音符吞掉。
#[test]
fn release_selection_skips_damper_pending_groups() {
    let voices = [
        test_voice(1, 60, 0, false),
        test_voice(2, 60, 0, true),
        test_voice(3, 60, 0, false),
    ];
    assert_eq!(
        select_release_note_id(&voices, [0, 1, 2]),
        Some(1),
        "最老的未释放组应优先"
    );

    // 组 1 已进入释放后，跳过的目标换成组 3（组 2 仍等待踏板）。
    let mut v1 = test_voice(1, 60, 0, false);
    v1.release_at = 10;
    let voices = [v1, test_voice(2, 60, 0, true), test_voice(3, 60, 0, false)];
    assert_eq!(select_release_note_id(&voices, [0, 1, 2]), Some(3));
}

#[test]
fn release_selection_returns_none_when_all_releasing_or_pending() {
    let mut v1 = test_voice(1, 60, 0, false);
    v1.released = true;
    let voices = [v1, test_voice(2, 60, 0, true)];
    assert_eq!(select_release_note_id(&voices, [0, 1]), None);
}

/// #42：踏板松开只释放"NoteOff 已到"的组；仍被按键按住的音符留在
/// 通道里继续发声，且其他通道不受影响。
#[test]
fn damper_release_selects_only_pending_groups_of_channel() {
    let voices = [
        test_voice(1, 60, 0, true),  // ch0：等待踏板 → 释放
        test_voice(2, 62, 0, false), // ch0：仍被按住 → 必须保留
        test_voice(3, 64, 1, true),  // ch1：另一个通道
        test_voice(1, 60, 0, true),  // ch0 同组第二个 zone → 去重
    ];
    assert_eq!(
        select_damper_release_groups(&voices, 0),
        vec![(60, 1)],
        "同一 note 组的多个 zone 只算一次"
    );
    assert_eq!(select_damper_release_groups(&voices, 1), vec![(64, 3)]);
}

#[test]
fn damper_release_ignores_released_or_ended_pending() {
    let mut a = test_voice(1, 60, 0, true);
    a.release_at = 5; // 已在释放
    let mut b = test_voice(2, 62, 0, true);
    b.state.ended = 1; // 已结束
    let c = test_voice(3, 64, 0, true);
    assert_eq!(select_damper_release_groups(&[a, b, c], 0), vec![(64, 3)]);
}

#[test]
fn evictions_never_steal_the_protected_group_when_candidates_run_short() {
    // 只有一个组就是保护组：宁可一个都不抢，也不能杀它（"新音符必发声"）。
    let only = [(0, 10, 7)];
    assert!(select_evictions(&only, 1, Some(7)).is_empty());
    assert!(select_evictions(&only, 9, Some(7)).is_empty());
    // 候选只有 1 个却要 5 个：抢到候选用尽即停，保护组（8）不受影响。
    let groups = [(0, 10, 7), (1, 20, 8)];
    assert_eq!(select_evictions(&groups, 5, Some(8)), vec![0]);
    // 保护 id 不在列表里（异常输入）：此时全部组都是普通候选。
    let groups = [(0, 10, 7), (1, 20, 8)];
    assert_eq!(select_evictions(&groups, 2, Some(99)), vec![0, 1]);
}
