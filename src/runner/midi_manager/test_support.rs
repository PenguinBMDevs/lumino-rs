//! `midi_manager` 测试用最小 `MidiApi` 替身（零驱动）。
//!
//! 供 `lgs_rebuild` / `layout` / 同步布局测试注入成功、Err、panic 三种行为；
//! 不触碰 WinMM / wgpu / 任何真实设备。

use std::sync::{Arc, Mutex};

use lumino_midi_io::{
    Api, Error, InputConnection, InputInfo, MidiInputCallback, OutputInfo, PlaybackOutput,
    SynthControl,
};
use lumino_midi_model::multi_port::PortSoundfontMap;

use super::MidiApi;

/// `rebuild_gpu_pipeline` 的行为。
#[derive(Clone, Copy)]
pub(super) enum RebuildBehavior {
    Ok,
    Panic(&'static str),
}

/// `set_midi_port_layout` 的行为。
#[derive(Clone, Copy)]
pub(super) enum LayoutBehavior {
    Ok,
    Panic(&'static str),
}

/// `apply_port_soundfonts` 的行为。
#[derive(Clone, Copy)]
pub(super) enum SoundfontBehavior {
    Ok,
    Err(&'static str),
    Panic(&'static str),
}

/// MockApi 调用记录：API 被 worker 暂持时仍可从测试侧读取。
#[derive(Clone, Default)]
pub(super) struct MockRecord {
    inner: Arc<Mutex<MockRecordData>>,
}

#[derive(Default)]
struct MockRecordData {
    layout_calls: Vec<u8>,
    soundfont_calls: Vec<PortSoundfontMap>,
}

impl MockRecord {
    pub(super) fn layout_calls(&self) -> Vec<u8> {
        self.inner
            .lock()
            .expect("读取 mock 布局记录")
            .layout_calls
            .clone()
    }

    pub(super) fn soundfont_calls(&self) -> Vec<PortSoundfontMap> {
        self.inner
            .lock()
            .expect("读取 mock 音色库记录")
            .soundfont_calls
            .clone()
    }
}

/// 最小 LGS API 替身。
pub(super) struct MockApi {
    rebuild: RebuildBehavior,
    layout: LayoutBehavior,
    soundfonts: SoundfontBehavior,
    record: Option<MockRecord>,
}

impl MockApi {
    pub(super) fn new(rebuild: RebuildBehavior, layout: LayoutBehavior) -> Self {
        Self {
            rebuild,
            layout,
            soundfonts: SoundfontBehavior::Ok,
            record: None,
        }
    }

    pub(super) fn ok() -> Self {
        Self::new(RebuildBehavior::Ok, LayoutBehavior::Ok)
    }

    pub(super) fn panic_on_rebuild(msg: &'static str) -> Self {
        Self::new(RebuildBehavior::Panic(msg), LayoutBehavior::Ok)
    }

    pub(super) fn panic_on_layout(msg: &'static str) -> Self {
        Self::new(RebuildBehavior::Ok, LayoutBehavior::Panic(msg))
    }

    /// 记录调用序列的替身（布局 / 端口音色库）。
    pub(super) fn recording(soundfonts: SoundfontBehavior) -> (Self, MockRecord) {
        let record = MockRecord::default();
        (
            Self {
                rebuild: RebuildBehavior::Ok,
                layout: LayoutBehavior::Ok,
                soundfonts,
                record: Some(record.clone()),
            },
            record,
        )
    }

    pub(super) fn boxed(self) -> MidiApi {
        Box::new(self)
    }
}

impl Api for MockApi {
    fn version(&self) -> Option<String> {
        Some("mock".to_string())
    }

    fn inputs(&self) -> Result<Vec<InputInfo>, Error> {
        Ok(Vec::new())
    }

    fn outputs(&self) -> Result<Vec<OutputInfo>, Error> {
        Ok(Vec::new())
    }

    fn open_output(&self, _id: u32) -> Result<Box<dyn PlaybackOutput>, Error> {
        Err(Error::OpenOutputFailed("mock 不支持输出".to_string()))
    }

    fn open_input(
        &self,
        _id: u32,
        _callback: MidiInputCallback,
    ) -> Result<Box<dyn InputConnection>, Error> {
        Err(Error::OpenInputFailed("mock 不支持输入".to_string()))
    }
}

impl SynthControl for MockApi {
    fn set_midi_port_layout(&mut self, max_port: u8) -> Result<(), String> {
        if let Some(record) = &self.record {
            record
                .inner
                .lock()
                .expect("写入 mock 布局记录")
                .layout_calls
                .push(max_port);
        }
        match self.layout {
            LayoutBehavior::Ok => Ok(()),
            LayoutBehavior::Panic(msg) => panic!("{msg}"),
        }
    }

    fn apply_port_soundfonts(&mut self, map: &PortSoundfontMap) -> Result<(), String> {
        if let Some(record) = &self.record {
            record
                .inner
                .lock()
                .expect("写入 mock 音色库记录")
                .soundfont_calls
                .push(map.clone());
        }
        match self.soundfonts {
            SoundfontBehavior::Ok => Ok(()),
            SoundfontBehavior::Err(msg) => Err(msg.to_string()),
            SoundfontBehavior::Panic(msg) => panic!("{msg}"),
        }
    }

    fn rebuild_gpu_pipeline(&mut self) -> Result<(), String> {
        match self.rebuild {
            RebuildBehavior::Ok => Ok(()),
            RebuildBehavior::Panic(msg) => panic!("{msg}"),
        }
    }
}
