//! CC 控制器柱状图渲染器
//!
//! 使用 GPU 实例化渲染绘制 MIDI CC 事件的垂直柱状条。
//! 与 yinhe 的自动化渲染方式一致：
//! - 每根柱子 2px 宽
//! - 高度 = value / 127 * panel_height
//! - 底部对齐（value 0 = 面板底部，value 127 = 面板顶部）
//! - CPU 计算屏幕坐标，GPU 直接绘制

mod core;
mod draw;
mod prepare;

pub use core::{
    CcBarColors, CcBarData, CcBarInstance, CcBarRenderer, CcBarViewParams, CcBarViewportUniform,
};
pub use prepare::build_cc_bar_instances;
