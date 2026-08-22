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
    /// 时间轴水平滚动（绝对滚动位置，来自滚动条拖拽/滚轮）
    TimelineScroll {
        /// 目标滚动位置（像素）
        x: f32,
        /// 发出时的时间轴可视宽度（用于钳制，避免视图状态反向同步）
        viewport_w: f32,
    },
    /// 时间轴缩放（目标倍率 + 锚点比例，来自滚动条边缘手势/滚轮）
    TimelineZoom {
        /// 目标缩放倍率
        zoom: f32,
        /// 锚点在视口内的横向比例（0.0 贴左，1.0 贴右）
        fixed_ratio: f32,
        /// 发出时的时间轴可视宽度（用于锚点换算与钳制）
        viewport_w: f32,
    },
    /// 时间轴标尺定位（点击/拖拽标尺移动剪辑面板播放头，秒域）
    ///
    /// 写入剪辑面板**独立传输时钟**，与卷帘 PlaybackManager 无关。
    TimelineSeek {
        /// 目标播放位置（秒）
        secs: f32,
    },
    /// 剪辑面板独立传输：切换播放/暂停
    ClipPlayToggled,
    /// 剪辑面板独立传输：回零并停止
    ClipRewound,
    /// 素材整体偏移变化（拖拽轨道条移动，绝对值无漂移）
    ClipTrackOffsetChanged {
        /// 目标轨道
        track: ClipTrack,
        /// 新的整体偏移（秒）
        offset_secs: f32,
    },
    /// 素材首尾裁剪变化（拖拽两端把手，绝对值）
    ClipTrimChanged {
        /// 目标轨道
        track: ClipTrack,
        /// 裁剪端
        edge: ClipTrimEdge,
        /// 新的裁剪量（秒）
        trim_secs: f32,
    },
    /// 预览区尺寸变化（responsive 回调）
    PreviewSizeChanged {
        /// 宽度
        width: f32,
        /// 高度
        height: f32,
    },
}

/// 剪辑轨道种类（视频/音频双轨）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipTrack {
    /// 视频轨
    Video,
    /// 音频轨
    Audio,
}

/// 素材裁剪端（首/尾）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipTrimEdge {
    /// 首端（trim in）
    Start,
    /// 尾端（trim out）
    End,
}
