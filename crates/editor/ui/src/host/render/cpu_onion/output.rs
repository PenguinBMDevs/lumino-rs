//! `CpuOnionState::update` 的产出，及其到 gfx 帧字段的映射（纯数据契约）。
//!
//! 单独成文件的原因：脏区语义横跨「状态机 → 渲染侧」，把它固定在一处便于对齐；
//! 同时让 `state.rs` 只留决策与执行（守住单文件 < 400 行的硬门槛）。

use lumino_cpu_onion::CpuOnionRect;
use lumino_gfx::{CpuOnionFrame, CpuOnionUploadRect};

/// 一次 `update` 的产出。
pub(crate) struct CpuOnionOutput {
    /// 待叠加的帧（`None` = 不绘制）
    pub frame: Option<CpuOnionFrame>,
    /// 本帧是否重新光栅化（调用方据此置内容脏 → present 前等待）
    pub rebuilt: bool,
    /// 本帧脏区（**帧内像素坐标**）：`Some` = 只有该矩形被重新光栅化，渲染侧可
    /// 只上传该子矩形；`None` = 整帧重绘或复用帧（必须按全量语义处理）。
    pub dirty: Option<CpuOnionRect>,
}

/// CPU 光栅脏区 → gfx 帧内矩形。
///
/// 二者坐标系完全一致（都是**帧内像素**，左上角 + 尺寸），这里只做字段搬运，
/// 不参与屏幕坐标换算——`CpuOnionFrame.x/y`（四边形屏幕位置）与此无关。
pub(crate) fn to_gfx_rect(rect: CpuOnionRect) -> CpuOnionUploadRect {
    CpuOnionUploadRect::new(rect.x, rect.y, rect.width, rect.height)
}
