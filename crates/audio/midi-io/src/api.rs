//! 后端 `Api` 实现汇总：KDMAPI、系统 MIDI、XSynth、LGS、Maestro。

pub mod kdmapi;
/// 文档切换时的布局过渡决策（XSynth/LGS 共用纯逻辑）
pub(crate) mod layout;
/// LGS (GPU) 软件合成后端
pub mod lgs;
/// Maestro 合成器宿主后端（AUD-009；Android 运行期返回“不支持”）
pub mod maestro;
/// 系统 MIDI 后端（依赖 midir；Android 无该后端，整体不参与编译）
#[cfg(not(target_os = "android"))]
pub mod system;
/// XSynth 软件合成后端
pub mod xsynth;
pub(crate) mod xsynth_output;

pub use kdmapi::Kdmapi;
pub use lgs::{Lgs, LgsOptions};
pub use maestro::{Maestro, MaestroStats, MaestroStatsReader};
#[cfg(not(target_os = "android"))]
pub use system::System;
pub use xsynth::{XSynth, XSynthOptions, XSynthStats};
