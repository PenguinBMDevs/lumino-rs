//! 后端 `Api` 实现汇总：KDMAPI、系统 MIDI、XSynth。

pub mod kdmapi;
/// 系统 MIDI 后端
pub mod system;
/// XSynth 软件合成后端
pub mod xsynth;
pub(crate) mod xsynth_output;
/// LGS (GPU) 软件合成后端
pub mod lgs;

pub use kdmapi::Kdmapi;
pub use system::System;
pub use xsynth::{XSynth, XSynthOptions, XSynthStats};
pub use lgs::{Lgs, LgsOptions};
