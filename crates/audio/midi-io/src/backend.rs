//! 音频后端双路线抽象
//!
//! 提供 `Realtime（lumino 原有，依赖 xsynth-realtime/cpal）` 与
//! `Core（yinhe 复刻，基于 xsynth-core + 自研 SPSC ring，无 cpal 阻塞）` 的统一工厂。
//! 后续 `Core` 的真实渲染在 `core_backend.rs` 实现，此处先以最小可用形态落地，保证 `2/4` 可编译。

use std::path::PathBuf;

use crate::{Api, Error, OutputConnection};

/// 后端类型，供 UI 配置与工厂选择
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    /// 实时路线：`xsynth-realtime` 内部多线程 + `BufferedRenderer` + `cpal` 回调
    Realtime,
    /// 核心路线：`xsynth-core ChannelGroup` + `AudioRing` + 独立渲染/工作线程，零锁回调
    Core,
}

impl BackendKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Realtime => "realtime",
            Self::Core => "core",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Realtime => "Realtime (xsynth)",
            Self::Core => "Core (xsynth-core + ring)",
        }
    }
    pub fn all() -> &'static [Self] {
        &[Self::Realtime, Self::Core]
    }
}

/// 创建指定后端的输出连接
///
/// `soundfont_path` 仅 Realtime/Core 均需；Core 若未就绪则回退为轻量空实现，保证调用方可无分支继续。
pub fn create_output(
    kind: BackendKind,
    soundfont_path: PathBuf,
    sample_rate: Option<u32>,
) -> Result<Box<dyn OutputConnection>, Error> {
    match kind {
        BackendKind::Realtime => create_realtime_output(soundfont_path, sample_rate),
        BackendKind::Core => create_core_output(soundfont_path, sample_rate),
    }
}

#[cfg(feature = "realtime")]
fn create_realtime_output(
    soundfont_path: PathBuf,
    sample_rate: Option<u32>,
) -> Result<Box<dyn OutputConnection>, Error> {
    use crate::api::xsynth::{XSynth, XSynthOptions};
    let opts = sample_rate.map(|sr| XSynthOptions {
        buffer_ms: 100.0,
        threads: 0,
        sample_rate: sr,
        fade_out_killing: true,
    });
    let api = XSynth::new(&soundfont_path, opts)?;
    api.open_output(0)
}

#[cfg(not(feature = "realtime"))]
fn create_realtime_output(
    _soundfont_path: PathBuf,
    _sample_rate: Option<u32>,
) -> Result<Box<dyn OutputConnection>, Error> {
    Err(Error::InitFailed(
        "realtime feature 未启用，无法创建 Realtime 后端".into(),
    ))
}

fn create_core_output(
    soundfont_path: PathBuf,
    sample_rate: Option<u32>,
) -> Result<Box<dyn OutputConnection>, Error> {
    crate::core_backend::CoreOutput::new(soundfont_path, sample_rate).map(|o| {
        let b: Box<dyn OutputConnection> = Box::new(o);
        b
    })
}
