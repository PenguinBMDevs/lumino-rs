//! 混音台的主窗口镜像状态。
//!
//! 混音台本体是独立 OS 窗口（`DialogType::Mixer`）：`open` 表示用户是否希望
//! 它开着，由 Runner 每帧对账（`RunnerInner::sync_mixer_dialog`）——
//! 主窗口只保留这个开关与主音量，布局 / 滚动 / 拖拽全部归对话框窗口所有。
//!
//! 绘制与交互在 `lumino-ui-egui::mixer`，模型装配在
//! `root/view/egui_chrome/mixer.rs`。

use crate::sidebar::MIXER_DEFAULT_VOLUME;

/// 混音台主窗口镜像状态。
#[derive(Debug, Clone)]
pub(crate) struct MixerPanelState {
    /// 是否请求打开混音台独立窗口。
    pub open: bool,
    /// 混音台主音量（全局音量控制器），0..=127（127 对应 0 dB）。
    pub(crate) master_volume: u8,
}

impl Default for MixerPanelState {
    fn default() -> Self {
        Self {
            open: false,
            // 主音量出厂默认 100（与单轨默认一致）。
            master_volume: MIXER_DEFAULT_VOLUME,
        }
    }
}
