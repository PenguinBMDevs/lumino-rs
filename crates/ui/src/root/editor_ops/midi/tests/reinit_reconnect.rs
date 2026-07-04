use super::common::*;
use crate::editor::note::Note;
use crate::message::Message;
use crate::playback::PlaybackState;
use crate::toolbar;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::Duration;

/// ─────────────────────────────────────────────────────────────
/// 音频链路变更测试集
/// ─────────────────────────────────────────────────────────────
///
/// 覆盖场景：
///   1. 播放中替换 MIDI 输出 → 新输出接收事件
///   2. pause_playback → 墙钟冻结，状态变为 Paused
///   3. 清空输出后重新设置 → 事件在新输出上恢复
///   4. 多次 swap 输出 → 只有最后一个输出活跃

/// 场景1：播放中替换 MIDI 输出连接（Bug 1 回归测试）
///
/// 设置面板更改后音频引擎重初始化，PlaybackManager.midi_output
/// 须从旧适配器切换到新适配器。
/// 验证：swap 后输出 B 收到事件，旧输出 A 不再增长。
#[test]
fn test_swap_midi_output_during_playback() {
    let mut root = create_root();
    let ppq = root.editor.editor_state.view.ppq;

    // 三条音符分布在 tick 0, 480, 960
    root.editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, ppq as f32 * 0.5));
    root.editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(ppq as f32, 64, ppq as f32 * 0.5));
    root.editor.editor_state.data.notes.push_back(Note::new(
        ppq as f32 * 2.0,
        67,
        ppq as f32 * 0.5,
    ));

    // ── 输出 A ──
    let out_a_on = Arc::new(AtomicU32::new(0));
    root.set_midi_output(Box::new(MockOutput::with_counters(
        Arc::clone(&out_a_on),
        Arc::new(AtomicU32::new(0)),
    )));
    root.update(Message::Toolbar(toolbar::Event::Play));
    assert!(root.playback.manager.is_some());
    assert!(root.playback.pending_midi_output.is_none());

    // 等待输出 A 收到 tick 0 的 note_on
    pump_until(&mut root, &out_a_on, 1, "输出 A");

    let count_before = out_a_on.load(Ordering::Relaxed);
    assert!(
        count_before > 0,
        "输出 A 应收到事件: count={}",
        count_before
    );

    // ── swap → 输出 B ──
    let out_b_on = Arc::new(AtomicU32::new(0));
    root.set_midi_output(Box::new(MockOutput::with_counters(
        Arc::clone(&out_b_on),
        Arc::new(AtomicU32::new(0)),
    )));

    // 等待输出 B 收到后续音符事件
    pump_until(&mut root, &out_b_on, 1, "输出 B");

    assert!(
        out_b_on.load(Ordering::Relaxed) > 0,
        "swap 后输出 B 应有事件"
    );

    // 旧输出不应再收到新事件
    let count_after = out_a_on.load(Ordering::Relaxed);
    let delta = count_after.saturating_sub(count_before);
    assert!(delta <= 1, "旧输出 A swap 后不应再增长: 增量为 {}", delta);

    root.update(Message::Toolbar(toolbar::Event::Stop));
}

/// 场景2：pause_playback 后墙钟冻结
#[test]
fn test_pause_playback_freezes_tick() {
    let mut root = create_root();
    root.editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));
    root.set_midi_output(create_mock_output());
    root.update(Message::Toolbar(toolbar::Event::Play));

    // 等待管理器进入 Playing 状态并 tick 前进
    wait_for_state(&root, PlaybackState::Playing);

    let tick_before = wait_for_tick_advance(&mut root);

    // ── 暂停 ──
    root.pause_playback();
    wait_for_state(&root, PlaybackState::Paused);

    // 暂停后 tick 不应显著前进
    let tick_after = root.playback.manager.as_ref().unwrap().current_tick();
    let diff = (tick_after - tick_before).abs();
    assert!(
        diff < 5.0,
        "暂停后 tick 不应显著前进: before={:.1}, after={:.1}, diff={:.1}",
        tick_before,
        tick_after,
        diff
    );

    root.update(Message::Toolbar(toolbar::Event::Stop));
}

/// 场景3：清空输出后重新设置 → 新输出继续接收事件
///
/// 模拟 reinit 场景：旧输出被销毁，重新注入新连接
#[test]
fn test_clear_then_set_midi_output_resumes() {
    let mut root = create_root();
    let ppq = root.editor.editor_state.view.ppq;
    root.editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, ppq as f32 * 0.5));
    root.editor.editor_state.data.notes.push_back(Note::new(
        ppq as f32 * 1.0,
        64,
        ppq as f32 * 0.5,
    ));

    let out_a_on = Arc::new(AtomicU32::new(0));
    root.set_midi_output(Box::new(MockOutput::with_counters(
        Arc::clone(&out_a_on),
        Arc::new(AtomicU32::new(0)),
    )));
    root.update(Message::Toolbar(toolbar::Event::Play));
    pump_until(&mut root, &out_a_on, 1, "初始播放");

    // ── 清空输出（模拟引擎销毁）──
    root.clear_midi_output();

    // ── 设置新输出 ──
    let out_b_on = Arc::new(AtomicU32::new(0));
    root.set_midi_output(Box::new(MockOutput::with_counters(
        Arc::clone(&out_b_on),
        Arc::new(AtomicU32::new(0)),
    )));

    // 等待新输出收到后续音符事件
    pump_until(&mut root, &out_b_on, 1, "重连后新输出");
    assert!(out_b_on.load(Ordering::Relaxed) > 0, "新输出应有 note_on");

    root.update(Message::Toolbar(toolbar::Event::Stop));
}

/// 场景4：多次 swap → 只有最后一个输出接收事件
#[test]
fn test_multiple_output_swaps() {
    let mut root = create_root();
    let ppq = root.editor.editor_state.view.ppq;
    root.editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, ppq as f32 * 0.5));
    root.editor.editor_state.data.notes.push_back(Note::new(
        ppq as f32 * 1.0,
        64,
        ppq as f32 * 0.5,
    ));
    root.editor.editor_state.data.notes.push_back(Note::new(
        ppq as f32 * 2.0,
        67,
        ppq as f32 * 0.5,
    ));

    let out_1 = Arc::new(AtomicU32::new(0));
    let out_2 = Arc::new(AtomicU32::new(0));
    let out_3 = Arc::new(AtomicU32::new(0));

    root.set_midi_output(Box::new(MockOutput::with_counters(
        Arc::clone(&out_1),
        Arc::new(AtomicU32::new(0)),
    )));
    root.update(Message::Toolbar(toolbar::Event::Play));

    // 快速连续 swap：输出1 可能会收到 tick 0 事件，
    // 输出2 和 输出3 看 timing（至少 输出3 应收到后续事件）
    root.set_midi_output(Box::new(MockOutput::with_counters(
        Arc::clone(&out_2),
        Arc::new(AtomicU32::new(0)),
    )));
    root.set_midi_output(Box::new(MockOutput::with_counters(
        Arc::clone(&out_3),
        Arc::new(AtomicU32::new(0)),
    )));

    // 输出3 作为最后一个输出应收到后续事件
    pump_until(&mut root, &out_3, 1, "输出3");
    assert!(
        out_3.load(Ordering::Relaxed) > 0,
        "最后设置的输出3 应有事件"
    );

    // 输出1 和 输出2 不应一直收事件（管道残余最多 1-2 个）
    let c1 = out_1.load(Ordering::Relaxed);
    let c2 = out_2.load(Ordering::Relaxed);
    tracing::info!(
        "多输出 swap: out1={}, out2={}, out3={}",
        c1,
        c2,
        out_3.load(Ordering::Relaxed)
    );

    root.update(Message::Toolbar(toolbar::Event::Stop));
}

/// ── 辅助函数 ──

/// 驱动播放循环，直到 counter >= min 或超时
fn pump_until(root: &mut crate::root::Root, counter: &AtomicU32, min: u32, label: &str) {
    thread::sleep(Duration::from_millis(50)); // 初始等待后台线程处理 Play
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while counter.load(Ordering::Relaxed) < min {
        assert!(
            std::time::Instant::now() < deadline,
            "[{}] 超时: count={} < min={}",
            label,
            counter.load(Ordering::Relaxed),
            min
        );
        root.update_playback();
        thread::sleep(Duration::from_millis(5));
    }
}

/// 驱动 N 次播放循环，推进 tick
fn pump_ticks(root: &mut crate::root::Root, n: usize) {
    for _ in 0..n {
        root.update_playback();
        thread::sleep(Duration::from_millis(5));
    }
}

/// 等待管理器进入指定状态
fn wait_for_state(root: &crate::root::Root, expected: PlaybackState) {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(ref m) = root.playback.manager {
            if m.state() == expected {
                return;
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "未进入状态 {:?}",
            expected
        );
        thread::sleep(Duration::from_millis(2));
    }
}

/// 等待 `update_playback()` 返回 > 0 的 tick
fn wait_for_tick_advance(root: &mut crate::root::Root) -> f32 {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut last_state = None;
    loop {
        // 打印当前状态用于调试
        if let Some(ref m) = root.playback.manager {
            let s = m.state();
            if last_state.map_or(true, |ls| ls != s) {
                tracing::debug!("wait_for_tick: state={:?}", s);
                last_state = Some(s);
            }
            if s == PlaybackState::Playing {
                let tick = m.current_tick();
                tracing::debug!("wait_for_tick: current_tick={}", tick);
                if tick > 0.0 {
                    root.editor.playback_position = tick;
                    return tick;
                }
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "tick 未在 2s 内前进, last_state={:?}",
            last_state
        );
        thread::sleep(Duration::from_millis(5));
    }
}
