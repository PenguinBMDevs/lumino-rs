//! MIDI 播放相关测试
//!
//! 测试覆盖：
//! - 录制过程中播放位置随时间推进
//! - MIDI API 设备枚举与缓存

use lumino_core::storage::config::UiConfig;
use lumino_ui::message::Message;
use lumino_ui::root::Root;
use lumino_ui::toolbar;

use crate::basic::MockMidiApi;

/// 测试 8：录制与播放位置的关系
///
/// 验证：录制过程中 playback_position 随时间推进
#[test]
fn test_recording_position_advancement() {
    let mut root = Root::new(&UiConfig::default());
    root.midi.api = Some(Box::new(MockMidiApi::new()));

    root.update(Message::Toolbar(toolbar::Event::Record));

    // 第一次 poll（设置 started_at）
    root.poll_midi_input();
    let pos1 = root.editor.playback_position;

    // 等待一小段时间后再次 poll
    std::thread::sleep(std::time::Duration::from_millis(50));
    root.poll_midi_input();
    let pos2 = root.editor.playback_position;

    assert!(pos2 > pos1, "录制位置应随时间推进 ({} -> {})", pos1, pos2);

    root.update(Message::Toolbar(toolbar::Event::RecordStop));
}

/// 测试 9：MIDI API 设备列表
///
/// 验证：set_midi_api 后设置面板正确缓存设备列表
#[test]
fn test_midi_api_device_enumeration() {
    let mut root = Root::new(&UiConfig::default());

    // 设置前设备列表为空
    assert!(
        root.settings().midi_devices.is_empty(),
        "初始设备列表应为空"
    );

    // 设置 Mock API
    root.set_midi_api(Box::new(MockMidiApi::new()));

    // 验证设备列表已缓存
    assert_eq!(root.settings().midi_devices.len(), 2, "应缓存 2 个输入设备");
    assert_eq!(
        root.settings().midi_devices[0].1,
        "Mock MIDI Keyboard",
        "设备名称应正确"
    );

    // 验证默认选中第一个设备
    assert_eq!(
        root.settings().selected_midi_device,
        Some(0),
        "应自动选中第一个设备"
    );
}
