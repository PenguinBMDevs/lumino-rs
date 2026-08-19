//! midiplayer —— MIDI 播放与可视化核心库
//!
//! 贴图瀑布流是当前已实现的功能之一，未来将在此基础上扩展
//! MIDI 播放、实时可视化等更多功能。
//!
//! 当前功能模块：
//! - [`texture_waterfall`]：贴图瀑布流 —— 按时间组/音轨组生成贴图、
//!   视口驱动流式加载与 LRU 淘汰的高性能贴图渲染系统。

pub mod texture_waterfall;
