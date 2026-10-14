//! #149：MIDI 设备扫描后台化——守卫 / 轮询回写 / 自动选中
//!
//! 扫描本身在后台线程执行：此处覆盖确定性的守卫逻辑与轮询写回行为；
//! System 后端只验证"在途状态已建立 + 可安全收尾"，不依赖真实设备列表。

use super::common::create_root;
use lumino_core::storage::config::SynthBackend;
use std::sync::mpsc;

/// 非 System 后端：扫描请求被守卫拦截（不建立任何在途扫描）。
#[test]
fn test_winmm_scan_guard_skips_non_system_backend() {
    let mut root = create_root();
    assert_eq!(
        root.settings.synth.backend,
        SynthBackend::XSynth,
        "默认配置应为 XSynth 后端"
    );

    root.scan_winmm_outputs();

    assert!(
        root.pending_winmm_scan.is_none(),
        "非 System 后端不得发起 WinMM 播表扫描"
    );
}

/// System 后端：扫描转入后台（在途状态建立），轮询可安全收尾。
#[test]
fn test_winmm_scan_system_backend_goes_async() {
    let mut root = create_root();
    root.settings.synth.backend = SynthBackend::System;

    root.scan_winmm_outputs();
    assert!(
        root.pending_winmm_scan.is_some(),
        "System 后端应发起后台扫描"
    );

    // 收尾：等待后台完成并轮询（最多 2 秒；线程异常断开同样清除在途状态）
    for _ in 0..200 {
        root.poll_winmm_scan();
        if root.pending_winmm_scan.is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(root.pending_winmm_scan.is_none(), "轮询应最终清除在途状态");
}

/// 扫描结果写回：设备列表更新 + 失效选中回落到第一个。
#[test]
fn test_poll_winmm_scan_writes_back_and_falls_back_selection() {
    let mut root = create_root();
    root.settings.midi.selected_winmm_output = Some(99); // 已失效的设备

    let (tx, rx) = mpsc::channel();
    root.pending_winmm_scan = Some(rx);
    tx.send(vec![
        (3, "VirtualMIDISynth".to_string()),
        (5, "GS Wavetable".to_string()),
    ])
    .expect("发送扫描结果");

    root.poll_winmm_scan();

    assert!(root.pending_winmm_scan.is_none(), "轮询后应清除在途状态");
    assert_eq!(root.settings.midi.winmm_outputs.len(), 2);
    assert_eq!(
        root.settings.midi.selected_winmm_output,
        Some(3),
        "失效选中应回落到第一个设备"
    );
}

/// 输入设备扫描结果写回：列表更新 + 自动选中首个。
#[test]
fn test_poll_midi_devices_scan_writes_back_and_auto_selects() {
    let mut root = create_root();
    assert!(root.settings.midi.selected_device.is_none());

    let (tx, rx) = mpsc::channel();
    root.pending_midi_devices_scan = Some(rx);
    tx.send(vec![(7, "loopMIDI Port".to_string())])
        .expect("发送扫描结果");

    root.poll_midi_devices_scan();

    assert!(root.pending_midi_devices_scan.is_none());
    assert_eq!(
        root.settings.midi.devices,
        vec![(7, "loopMIDI Port".to_string())]
    );
    assert_eq!(
        root.settings.midi.selected_device,
        Some(7),
        "应自动选中首个输入设备"
    );
}

/// 未注入 API：输入设备扫描直接置空列表，不建立在途状态。
#[test]
fn test_midi_devices_scan_without_api_clears_list() {
    let mut root = create_root();
    root.settings.midi.devices = vec![(1, "Stale".to_string())];

    root.scan_midi_input_devices();

    assert!(
        root.pending_midi_devices_scan.is_none(),
        "无 API 不应发起扫描"
    );
    assert!(
        root.settings.midi.devices.is_empty(),
        "无 API 时应置空设备列表"
    );
}
