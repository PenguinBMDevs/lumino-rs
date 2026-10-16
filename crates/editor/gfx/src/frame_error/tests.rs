//! R0：`handle_frame_error` 对四种 `wgpu::SurfaceError` 的分类表驱动测试。

use wgpu::SurfaceError;

use super::{FrameErrorAction, classify_frame_error};

/// 分类表：四种错误的动作 + 是否安排重绘（单一权威期望表）。
#[test]
fn test_r0_frame_error_classification_table() {
    let table = [
        // (错误, 期望动作, 期望 should_redraw)
        (SurfaceError::Outdated, FrameErrorAction::Reconfigured, true),
        (SurfaceError::Timeout, FrameErrorAction::SkipFrame, true),
        (SurfaceError::Lost, FrameErrorAction::Stop, false),
        (SurfaceError::OutOfMemory, FrameErrorAction::Stop, false),
        // wgpu 27 第 5 变体：未归类瞬时错误 → 按可恢复处理
        (SurfaceError::Other, FrameErrorAction::SkipFrame, true),
    ];
    for (err, expected, redraw) in table {
        let action = classify_frame_error(&err);
        assert_eq!(action, expected, "SurfaceError::{err:?} 分类错误");
        assert_eq!(
            action.should_redraw(),
            redraw,
            "SurfaceError::{err:?} 的 should_redraw 判定错误"
        );
    }
}

/// Stop 必须拒绝安排重绘（§1.6：禁止对 Lost/OOM 盲 `request_redraw` 死循环）。
#[test]
fn test_r0_stop_never_requests_redraw() {
    assert!(!FrameErrorAction::Stop.should_redraw());
    assert!(FrameErrorAction::Reconfigured.should_redraw());
    assert!(FrameErrorAction::SkipFrame.should_redraw());
}
