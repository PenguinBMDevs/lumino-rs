//! 浮动混音台面板的**状态**（视图已迁到 egui）。
//!
//! 绘制与输入现在由 `lumino-ui-egui::mixer` 负责（`egui_chrome/mixer.rs` 装配模型）：
//! 面板矩形按窗口尺寸锚定在左下角、状态栏之上，通道条 / 推子 / 电平 / P·M·S
//! 全部由 egui 绘制，`MixerAction` 映射回 `Event::TrackGainChanged` 等既有消息。
//!
//! 这里只保留 **UI 无关的状态**：开关、展开、拖拽偏移、主音量、横向滚动。
//! iced 侧的 `view_mixer_panel` / `strips` 视图已删除（不再有 iced 混音台）。

use crate::sidebar::MIXER_DEFAULT_VOLUME;

/// 浮动混音台面板状态
#[derive(Debug, Clone)]
pub(crate) struct MixerPanelState {
    /// 面板是否打开
    pub open: bool,
    /// 面板主体是否展开（最大化）；false = 仅显示标题栏（最小化）
    pub maximized: bool,
    /// 面板距左/底边界内缩（拖拽累加，逻辑像素）。
    /// `offset.0` = 距左边界内缩（= 面板左缘 x），`offset.1` = 距底边界内缩。
    pub offset: (f32, f32),
    /// 是否正在拖拽（标题栏按下且未松开）
    pub(crate) dragging: bool,
    /// 拖拽期间上一帧的绝对光标位置（相对全窗口覆盖层）；用于计算增量，
    /// 使面板在光标离开标题栏/窗口范围时仍跟随（首次 move 时为 None 仅记录）。
    pub(crate) last_cursor: Option<(f32, f32)>,
    /// 混音台主音量（全局音量控制器），0..=127（127 对应 0 dB）
    pub(crate) master_volume: u8,
    /// 混音台主体横向滚动偏移（逻辑像素），用于视口裁剪节流
    pub(crate) scroll_x: f32,
}

impl Default for MixerPanelState {
    fn default() -> Self {
        Self {
            open: false,
            // 默认展开主体（显示各音轨推子）；仅标题栏的紧凑态需用户收起。
            maximized: true,
            // 默认显形位置：紧贴左侧栏右侧（48px 栏宽 + 8px 间隙）、距底 8px，
            // 避免渲染在 (0,0) 被左栏与状态栏遮住而"看起来没弹出"。
            offset: (56.0, 8.0),
            dragging: false,
            last_cursor: None,
            // 主音量出厂默认 100（与单轨默认一致）。
            master_volume: MIXER_DEFAULT_VOLUME,
            scroll_x: 0.0,
        }
    }
}
