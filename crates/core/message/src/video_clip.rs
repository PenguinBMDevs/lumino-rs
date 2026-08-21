//! 视频剪辑面板消息（瀑布流预览交互）
//!
//! 对标 nezha `piano_view::show` 的 zoom / pan 交互。

/// 视频剪辑操作
#[derive(Debug, Clone)]
pub enum VideoClipAction {
    /// 缩放变化（由滚轮/手势触发，factor 为乘数）
    ZoomChanged(f32),
    /// 直接设置缩放
    ZoomSet(f32),
    /// 平移（dx, dy）
    PanChanged {
        /// 水平增量
        dx: f32,
        /// 垂直增量
        dy: f32,
    },
    /// 以锚点为中心缩放（带光标位置）
    ZoomAround {
        /// 旧缩放
        old_zoom: f32,
        /// 新缩放
        new_zoom: f32,
        /// 光标位置 x
        cursor_x: f32,
        /// 光标位置 y
        cursor_y: f32,
        /// 视口中心 x
        center_x: f32,
        /// 视口中心 y
        center_y: f32,
    },
    /// 重置视口（双击）
    ResetView,
    /// 预览区尺寸变化（responsive 回调）
    PreviewSizeChanged {
        /// 宽度
        width: f32,
        /// 高度
        height: f32,
    },
}
