//! 后端 `Api` 实现汇总：KDMAPI、系统 MIDI、XSynth。

pub mod kdmapi;
/// 文档切换时的布局过渡决策（XSynth/LGS 共用纯逻辑）
pub(crate) mod layout;
/// LGS (GPU) 软件合成后端
pub mod lgs;
/// 系统 MIDI 后端（依赖 midir；Android 无该后端，整体不参与编译）
#[cfg(not(target_os = "android"))]
pub mod system;
/// XSynth 软件合成后端
pub mod xsynth;
pub(crate) mod xsynth_output;

pub use kdmapi::Kdmapi;
pub use lgs::{Lgs, LgsOptions};
#[cfg(not(target_os = "android"))]
pub use system::System;
pub use xsynth::{XSynth, XSynthOptions, XSynthStats};
