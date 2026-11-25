//! REND-005：端口音色库延迟应用状态机测试。
//!
//! 使用 `test_support::MockApi`（零驱动）：只验证“何时调用后端、是否触碰布局、
//! 失败后是否反复重试”，不触碰真实音频设备。

use std::time::{Duration, Instant};

use lumino_core::storage::config::SynthBackend;
use lumino_midi_model::multi_port::PortSoundfontMap;

use super::super::test_support::{MockApi, SoundfontBehavior};
use crate::runner::midi_manager::{MidiApi, MidiManager};

fn map_with(entries: &[(u8, &[&str])]) -> PortSoundfontMap {
    let mut map = PortSoundfontMap::default();
    for (port, paths) in entries {
        map.set(
            *port,
            paths.iter().map(|path| (*path).to_string()).collect(),
        );
    }
    map
}

fn manager_with(api: MidiApi) -> MidiManager {
    MidiManager {
        api: Some(api),
        active_backend: SynthBackend::XSynth,
        ..MidiManager::default()
    }
}

/// 轮询到 worker 收尾（上限 2s，避免测试挂死）。
fn poll_until_done(manager: &mut MidiManager) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while manager.layout_apply.is_some() && Instant::now() < deadline {
        manager.poll_layout_apply();
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(manager.layout_apply.is_none(), "worker 未在 2s 内收尾");
}

/// 空闲时改映射：只调用 `apply_port_soundfonts`，绝不触碰布局。
///
/// 同布局的 `set_midi_port_layout` 会走轻量复位（`SystemReset` + 杀音），
/// 这是 REND-005 明确禁止的行为。
#[test]
fn idle_mapping_applies_without_layout_reset() {
    let (api, record) = MockApi::recording(SoundfontBehavior::Ok);
    let mut manager = manager_with(api.boxed());
    let map = map_with(&[(1, &["port1.sf2"])]);

    manager.apply_port_soundfonts_deferred(map.clone());
    poll_until_done(&mut manager);

    assert_eq!(record.soundfont_calls(), vec![map.clone()]);
    assert!(
        record.layout_calls().is_empty(),
        "纯映射变化不得触发布局轻量复位（会杀音）"
    );
    assert_eq!(manager.spawned_port_soundfonts, map);
}

/// 播放中提交映射：只暂存并提示，停播后才真正应用。
#[test]
fn playing_defers_until_playback_stops() {
    let (api, record) = MockApi::recording(SoundfontBehavior::Ok);
    let mut manager = manager_with(api.boxed());
    let map = map_with(&[(2, &["port2.sf2"])]);

    manager.tick_port_soundfonts(true);
    manager.apply_port_soundfonts_deferred(map.clone());

    assert!(manager.layout_apply.is_none(), "播放中不得排程后台应用");
    let notice = manager
        .take_pending_backend_notice()
        .expect("播放中应提示延迟生效");
    assert!(notice.contains("播放结束"), "提示应说明生效时机: {notice}");

    manager.tick_port_soundfonts(false);
    poll_until_done(&mut manager);
    assert_eq!(record.soundfont_calls(), vec![map]);
}

/// 布局 + 映射同帧变化：同一个 worker 依次应用，避免争抢 API。
#[test]
fn layout_and_mapping_apply_in_one_worker() {
    let (api, record) = MockApi::recording(SoundfontBehavior::Ok);
    let mut manager = manager_with(api.boxed());
    let map = map_with(&[(1, &["port1.sf2"])]);

    manager.apply_midi_port_layout_deferred(2);
    manager.apply_port_soundfonts_deferred(map.clone());
    poll_until_done(&mut manager);

    assert_eq!(record.layout_calls(), vec![2]);
    assert_eq!(record.soundfont_calls(), vec![map.clone()]);
    assert_eq!(manager.spawned_midi_max_port, 2);
    assert_eq!(manager.spawned_port_soundfonts, map);
}

/// 防抖窗口内开始播放：worker 执行时若在播放必须跳过映射。
#[test]
fn worker_skips_mapping_when_playback_starts_during_debounce() {
    let (api, record) = MockApi::recording(SoundfontBehavior::Ok);
    let mut manager = manager_with(api.boxed());
    let map = map_with(&[(1, &["port1.sf2"])]);

    manager.apply_port_soundfonts_deferred(map.clone());
    // worker 仍在 150ms 防抖窗口内；此时开始播放。
    manager.tick_port_soundfonts(true);
    poll_until_done(&mut manager);

    assert!(
        record.soundfont_calls().is_empty(),
        "worker 执行时在播放应跳过映射"
    );
    assert_eq!(
        manager.spawned_port_soundfonts,
        PortSoundfontMap::default(),
        "未应用不得提交"
    );

    manager.tick_port_soundfonts(false);
    poll_until_done(&mut manager);
    assert_eq!(record.soundfont_calls(), vec![map]);
}

/// 后端返回 Err（部分端口回退全局）：记录已应用 + 产生提示，不进入重试循环。
#[test]
fn apply_error_records_notice_and_avoids_retry_loop() {
    let (api, record) = MockApi::recording(SoundfontBehavior::Err("端口 1 音色库加载失败"));
    let mut manager = manager_with(api.boxed());
    let map = map_with(&[(1, &["broken.sf2"])]);

    manager.apply_port_soundfonts_deferred(map.clone());
    poll_until_done(&mut manager);

    assert_eq!(record.soundfont_calls().len(), 1);
    assert_eq!(
        manager.spawned_port_soundfonts, map,
        "回退全局也视为已落定，避免每帧重试"
    );
    let notice = manager
        .take_pending_backend_notice()
        .expect("应产生可见的回退提示");
    assert!(
        notice.contains("端口 1 音色库加载失败"),
        "提示应携带具体原因: {notice}"
    );

    // 空转一拍不得再次排程/调用。
    manager.tick_port_soundfonts(false);
    assert!(manager.layout_apply.is_none());
    assert_eq!(record.soundfont_calls().len(), 1);
}

/// 清空映射必须显式下发全局库（恢复覆盖过的端口）。
#[test]
fn clearing_mapping_restores_global() {
    let (api, record) = MockApi::recording(SoundfontBehavior::Ok);
    let mut manager = manager_with(api.boxed());
    let map = map_with(&[(0, &["port0.sf2"])]);

    manager.apply_port_soundfonts_deferred(map.clone());
    poll_until_done(&mut manager);
    manager.apply_port_soundfonts_deferred(PortSoundfontMap::default());
    poll_until_done(&mut manager);

    assert_eq!(
        record.soundfont_calls(),
        vec![map, PortSoundfontMap::default()],
        "清空映射必须显式下发恢复全局"
    );
    assert_eq!(manager.spawned_port_soundfonts, PortSoundfontMap::default());
}

/// worker 内 panic：API 必须归还，且不提交映射、触发后端重初始化。
#[test]
fn soundfont_panic_keeps_api_and_requests_reinit() {
    let (api, _) = MockApi::recording(SoundfontBehavior::Panic("注入的音色库 panic"));
    let mut manager = manager_with(api.boxed());

    manager.apply_port_soundfonts_deferred(map_with(&[(1, &["port1.sf2"])]));
    poll_until_done(&mut manager);

    assert!(manager.api.is_some(), "panic 后 API 必须归还");
    assert_eq!(
        manager.spawned_port_soundfonts,
        PortSoundfontMap::default(),
        "panic 不得提交映射"
    );
    assert!(manager.needs_reinit(), "worker panic 应触发后端重初始化");
}

/// 非软件后端（System）：给出可见提示，但不得排程无意义的后台任务。
#[test]
fn non_software_backend_reports_notice_without_scheduling() {
    let mut manager = MidiManager::default();
    assert_eq!(manager.active_backend, SynthBackend::System);

    manager.apply_port_soundfonts_deferred(map_with(&[(1, &["port1.sf2"])]));

    assert!(manager.layout_apply.is_none());
    let notice = manager
        .take_pending_backend_notice()
        .expect("不支持的应产生可见提示");
    assert!(notice.contains("不支持"), "提示应说明后端限制: {notice}");
}
