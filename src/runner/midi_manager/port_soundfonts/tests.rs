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

/// 非软件后端（System）：请求专用弹窗，但不得排程无意义的后台任务。
#[test]
fn non_software_backend_reports_dialog_without_scheduling() {
    let mut manager = MidiManager::default();
    assert_eq!(manager.active_backend, SynthBackend::System);
    let map = map_with(&[(1, &["port1.sf2"])]);

    manager.apply_port_soundfonts_deferred(map.clone());

    assert!(manager.layout_apply.is_none());
    let notice = manager
        .take_port_soundfonts_unsupported_notice()
        .expect("不支持的应请求专用弹窗");
    assert_eq!(notice.backend, SynthBackend::System);
    assert_eq!(notice.ports, 1);
    assert!(
        manager.take_pending_backend_notice().is_none(),
        "不支持不再写状态栏（改由弹窗告知）"
    );
    assert_eq!(
        manager.spawned_port_soundfonts, map,
        "降级应记录为已应用，避免 tick 反复排程"
    );
}

/// LGS 已就绪：同样请求弹窗、不排程 worker。
#[test]
fn lgs_ready_reports_dialog_without_worker() {
    let (api, record) = MockApi::recording(SoundfontBehavior::Ok);
    let mut manager = MidiManager {
        api: Some(api.boxed()),
        active_backend: SynthBackend::Lgs,
        ..MidiManager::default()
    };
    let map = map_with(&[(1, &["port1.sf2"])]);

    manager.apply_port_soundfonts_deferred(map);

    assert!(manager.layout_apply.is_none());
    assert!(
        record.soundfont_calls().is_empty(),
        "不支持的后端不得调用 apply_port_soundfonts"
    );
    let notice = manager
        .take_port_soundfonts_unsupported_notice()
        .expect("LGS 应请求专用弹窗");
    assert_eq!(notice.backend, SynthBackend::Lgs);
}

/// 弹窗请求每轮只发一次：用户关闭后不得每帧重复弹。
#[test]
fn unsupported_dialog_is_requested_once_per_round() {
    let mut manager = MidiManager::default();
    manager.apply_port_soundfonts_deferred(map_with(&[(1, &["port1.sf2"])]));
    assert!(manager.take_port_soundfonts_unsupported_notice().is_some());

    manager.tick_port_soundfonts(false);
    assert!(
        manager.take_port_soundfonts_unsupported_notice().is_none(),
        "同一轮映射/后端不得重复请求弹窗"
    );
}

/// 切到 XSynth 后（代际切换已清空 spawned/ack）应正常应用映射，且不再弹窗。
#[test]
fn switching_to_xsynth_applies_mapping() {
    let (api, record) = MockApi::recording(SoundfontBehavior::Ok);
    let mut manager = manager_with(api.boxed());
    let map = map_with(&[(1, &["port1.sf2"])]);

    // 模拟 reinit 后的状态：新后端 XSynth，spawned 清空、ack 重置、desired 保留。
    manager.spawned_port_soundfonts = PortSoundfontMap::default();
    manager.port_soundfonts_unsupported_ack = false;
    manager.desired_port_soundfonts = map.clone();
    *manager
        .port_soundfonts_desired
        .lock()
        .expect("锁端口音色库期望值") = map.clone();

    manager.tick_port_soundfonts(false);
    poll_until_done(&mut manager);

    assert_eq!(record.soundfont_calls(), vec![map.clone()]);
    assert_eq!(manager.spawned_port_soundfonts, map);
    assert!(
        manager.take_port_soundfonts_unsupported_notice().is_none(),
        "XSynth 支持按端口音色库，不得再请求不支持弹窗"
    );
}

/// 清空映射不得弹“不支持”窗。
#[test]
fn clearing_mapping_on_unsupported_backend_is_silent() {
    let mut manager = MidiManager::default();
    manager.apply_port_soundfonts_deferred(map_with(&[(1, &["port1.sf2"])]));
    assert!(manager.take_port_soundfonts_unsupported_notice().is_some());

    manager.apply_port_soundfonts_deferred(PortSoundfontMap::default());

    assert!(
        manager.take_port_soundfonts_unsupported_notice().is_none(),
        "清空映射后没有需要告知的不支持状态"
    );
    assert!(manager.spawned_port_soundfonts.is_empty());
}
