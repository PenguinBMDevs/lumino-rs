//! 返工 #149：输入 API 注入不得触发设备枚举（启动路径零 midir/WinMM 触碰）
//!
//! omv2 等环境下 WinMM 接口可能在原生层崩溃（AV——后台线程同样杀进程），
//! 因此 `set_midi_api` 只注入 API；设备枚举改由用户主动路径触发
//! （录制开始时自愈），或测试/后续接线显式调用 `scan_midi_input_devices`。

use super::common::create_root;
use lumino_midi_io::{
    Api, Error, InputConnection, InputInfo, MidiInputCallback, OutputInfo, PlaybackOutput,
};

/// 最小 Fake API：`inputs()` 返回固定 2 个设备，供验证"显式扫描"路径。
struct FakeApi;

impl Api for FakeApi {
    fn version(&self) -> Option<String> {
        Some("fake".to_string())
    }

    fn inputs(&self) -> Result<Vec<InputInfo>, Error> {
        Ok(vec![
            InputInfo {
                id: 7,
                name: "loopMIDI Port".to_string(),
            },
            InputInfo {
                id: 9,
                name: "Fake Keyboard".to_string(),
            },
        ])
    }

    fn outputs(&self) -> Result<Vec<OutputInfo>, Error> {
        Ok(Vec::new())
    }

    fn open_output(&self, _id: u32) -> Result<Box<dyn PlaybackOutput>, Error> {
        Err(Error::OpenOutputFailed("FakeApi 不支持输出".to_string()))
    }

    fn open_input(
        &self,
        _id: u32,
        _callback: MidiInputCallback,
    ) -> Result<Box<dyn InputConnection>, Error> {
        Err(Error::OpenInputFailed("FakeApi 不支持输入".to_string()))
    }
}

/// 返工 #149：`set_midi_api` 只注入 API，不得触发枚举或建立扫描在途状态。
#[test]
fn test_set_midi_api_does_not_trigger_device_scan() {
    let mut root = create_root();
    assert!(root.settings.midi.devices.is_empty(), "初始设备列表应为空");

    root.set_midi_api(Box::new(FakeApi));

    assert!(
        root.settings.midi.devices.is_empty(),
        "注入 API 后不得自动枚举设备（返工 #149：启动路径零触碰）"
    );
    assert!(
        root.pending_midi_devices_scan.is_none(),
        "注入 API 后不得建立后台扫描在途状态"
    );
}

/// 返工 #149：显式扫描路径（测试/后续接线备用）仍可用——后台线程
/// 枚举 + 轮询回写设备列表并自动选中首个。
#[test]
fn test_explicit_device_scan_still_works() {
    let mut root = create_root();
    root.set_midi_api(Box::new(FakeApi));

    root.scan_midi_input_devices();
    assert!(
        root.pending_midi_devices_scan.is_some(),
        "显式扫描应建立后台在途状态"
    );

    // 收尾：等待后台完成并轮询（最多 2 秒）
    for _ in 0..200 {
        root.poll_midi_devices_scan();
        if root.pending_midi_devices_scan.is_none() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    assert!(
        root.pending_midi_devices_scan.is_none(),
        "轮询应最终清除在途状态"
    );
    assert_eq!(
        root.settings.midi.devices.len(),
        2,
        "显式扫描应回写 2 个设备"
    );
    assert_eq!(
        root.settings.midi.devices[0].1, "loopMIDI Port",
        "设备名称应正确"
    );
    assert_eq!(
        root.settings.midi.selected_device,
        Some(7),
        "应自动选中首个设备"
    );
}
