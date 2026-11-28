//! 视频剪辑时间轴的 egui 实现（标尺 + 双轨条 + 走带线 + 命中测试）。
//!
//! 对应 上一代 `view/video_clip/timeline_canvas{,/draw}`：几何常量、刻度规则、
//! 圆角轨道条与首尾裁剪把手、命中区划分全部同口径。交互（scrub / 移动 / 裁剪）
//! 由调用方按 [`HitZone`] 分派，本模块只负责「画什么」与「点到哪」。
//!
//! 与 上一代 的差异：不依赖 画布对象，只用 egui painter。
//!
//! 结构：`model`（数据 / 几何 / 命中测试）、`paint`（标题行与时间轴绘制）、
//! `input`（按下 / 拖动 / 滚轮与拖拽会话）。对外仍从本模块根导出，调用方路径不变。

mod input;
mod model;
mod paint;

#[cfg(test)]
mod tests;

pub use input::{ClipEdge, DragSession, VideoTimelineAction, drag, interact, press, wheel};
pub use model::{
    ClipEdit, ClipTrack, HANDLE_HIT_WIDTH, HANDLE_WIDTH, HitZone, MAJOR_INTERVAL, MINOR_INTERVAL,
    PIXELS_PER_SEC, RULER_HEIGHT, TRACK_CORNER_RADIUS, TRACK_HEIGHT, TRACK_SPACING, TrackGeom,
    VideoTimelineModel, hit_test, ticks,
};
pub use paint::{show, show_title_row};
