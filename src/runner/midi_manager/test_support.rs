//! `midi_manager` 测试用最小 `MidiApi` 替身（零驱动）。
//!
//! 供 `lgs_rebuild` / `layout` / 同步布局测试注入成功、Err、panic 三种行为；
//! 不触碰 WinMM / wgpu / 任何真实设备。

use lumino_midi_io::{
    Api, Error, InputConnection, InputInfo, MidiInputCallback, OutputInfo, PlaybackOutput,
    SynthControl,
};

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

/// 最小 LGS API 替身。
pub(super) struct MockApi {
    rebuild: RebuildBehavior,
    layout: LayoutBehavior,
}

impl MockApi {
    pub(super) fn new(rebuild: RebuildBehavior, layout: LayoutBehavior) -> Self {
        Self { rebuild, layout }
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
    fn set_midi_port_layout(&mut self, _max_port: u8) -> Result<(), String> {
        match self.layout {
            LayoutBehavior::Ok => Ok(()),
            LayoutBehavior::Panic(msg) => panic!("{msg}"),
        }
    }

    fn rebuild_gpu_pipeline(&mut self) -> Result<(), String> {
        match self.rebuild {
            RebuildBehavior::Ok => Ok(()),
            RebuildBehavior::Panic(msg) => panic!("{msg}"),
        }
    }
}
