//! 视频导出帧参数、耗时统计与有界帧队列配置的数据类型。

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize};

/// 进度消息载荷：(文本, 进度 0..1, 总帧数, 平滑 FPS, 已用秒)
pub(super) type ProgressMsg = (String, f64, u64, f64, f64);

/// 流水线预填充深度：与渲染线程 4 槽 `StagingRing` 对齐。
///
/// 帧队列容量下限必须 ≥ 此值，否则预填充阶段会卡在满队列无法推进。
pub(super) const PIPELINE_DEPTH: usize = 4;

/// 单帧 BGRA/RGBA 字节数（4 字节/像素）。
#[inline]
pub(super) fn frame_byte_size(width: u32, height: u32) -> usize {
    (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4)
}

/// 有界帧队列容量配置。
///
/// 时间预算决定期望缓存帧数，内存预算是防止 300FPS 场景 OOM 的硬护栏
/// （1080p 单帧 8.29MB，未封顶缓存 2 秒 ≈ 4.6GiB）。
#[derive(Debug, Clone, Copy)]
pub(super) struct FrameQueueConfig {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) fps: f64,
    /// 最多缓存多少秒视频（视频时间，非墙钟）。
    pub(super) buffer_seconds: f64,
    /// 队列本身允许占用的最大字节数（不含在途帧）。
    pub(super) max_bytes: usize,
    /// 容量下限（流水线预填充深度）。
    pub(super) min_frames: usize,
}

impl FrameQueueConfig {
    /// 计算队列容量（帧数）：时间预算与内存预算取小，再保证不低于流水线深度。
    pub(super) fn capacity(&self) -> usize {
        let frame_bytes = frame_byte_size(self.width, self.height).max(1);
        let by_time = (self.buffer_seconds.max(0.0) * self.fps.max(0.0)).ceil() as usize;
        let by_mem = self.max_bytes / frame_bytes;
        by_time.min(by_mem).max(self.min_frames)
    }
}

/// 跨线程编码遥测（导出线程写 queue_depth，写线程写 encoded/encode_us）。
#[derive(Debug, Default)]
pub(super) struct EncodeTelemetry {
    /// 写线程已完成 `write_frame` 的帧数。
    pub(super) encoded_frames: AtomicU64,
    /// 写线程累计 `write_frame` 耗时（微秒）。
    pub(super) encode_us_total: AtomicU64,
    /// 当前帧队列深度（入队 +1 / 出队 -1）。
    pub(super) queue_depth: AtomicUsize,
    /// 写线程失败标志；导出线程主循环检测后立即终止。
    pub(super) failed: AtomicBool,
}

/// 单帧合成参数（与帧数据 FIFO 一一对应），替代裸 6 元组避免位置解构出错。
///
/// 键色不在此结构中：由渲染线程 GPU 活跃键内核推导后随 `VideoFrameMessage`
/// 同通道发送（保证键色 ↔ 帧严格同序）。
#[derive(Debug, Clone, Copy)]
pub(super) struct FrameParams {
    /// 标尺滚动偏移（像素）
    pub(super) scroll_x: f32,
    /// 标尺缩放（像素/tick）
    pub(super) zoom_x: f32,
    /// 键盘宽度（像素）
    pub(super) keyboard_width: f32,
    /// 分辨率（Pulses Per Quarter note）
    pub(super) ppq: u32,
}

impl Default for FrameParams {
    fn default() -> Self {
        Self {
            scroll_x: 0.0,
            zoom_x: 1.0,
            keyboard_width: 60.0,
            ppq: 0,
        }
    }
}

/// 编码帧参数队列（入队顺序与帧数据 FIFO 严格对应）。
pub(super) type EncodeFrameQueue = std::collections::VecDeque<FrameParams>;

/// 单帧处理阶段耗时统计（微秒）
#[derive(Debug, Default)]
pub(super) struct FrameStageStats {
    /// 键盘 + 标尺合成耗时
    pub(super) composite_us: u64,
    /// 预览帧克隆/缩放/发送耗时
    pub(super) preview_us: u64,
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::*;

    fn config(
        width: u32,
        height: u32,
        fps: f64,
        buffer_seconds: f64,
        max_bytes: usize,
    ) -> FrameQueueConfig {
        FrameQueueConfig {
            width,
            height,
            fps,
            buffer_seconds,
            max_bytes,
            min_frames: 4,
        }
    }

    #[test]
    fn test_frame_byte_size_1080p() {
        assert_eq!(frame_byte_size(1920, 1080), 8_294_400);
    }

    #[test]
    fn test_frame_byte_size_zero_dimension() {
        assert_eq!(frame_byte_size(0, 1080), 0);
        assert_eq!(frame_byte_size(1920, 0), 0);
    }

    #[test]
    fn test_queue_capacity_by_time_budget() {
        // 60fps × 2s = 120 帧，内存预算充足
        let c = config(1920, 1080, 60.0, 2.0, 4 * 1024 * 1024 * 1024);
        assert_eq!(c.capacity(), 120);
    }

    #[test]
    fn test_queue_capacity_capped_by_memory_budget() {
        // 300fps × 2s = 600 帧时间预算；512MiB 内存预算只能装 64 帧
        let c = config(1920, 1080, 300.0, 2.0, 512 * 1024 * 1024);
        let expected = 512 * 1024 * 1024 / 8_294_400;
        assert_eq!(c.capacity(), expected);
        assert!(c.capacity() < 600);
    }

    #[test]
    fn test_queue_capacity_floor_is_pipeline_depth() {
        // 内存预算连 1 帧都装不下 → 仍返回 min_frames，宁可多吃内存也不死锁流水线
        let c = config(1920, 1080, 60.0, 0.0, 1024);
        assert_eq!(c.capacity(), 4);
    }

    #[test]
    fn test_queue_capacity_fractional_rounds_up() {
        // 0.1 秒 × 60fps = 6 帧
        let c = config(640, 480, 60.0, 0.1, 1024 * 1024 * 1024);
        assert_eq!(c.capacity(), 6);
    }

    #[test]
    fn test_telemetry_atomic_counts() {
        let t = EncodeTelemetry::default();
        t.queue_depth.fetch_add(1, Ordering::Relaxed);
        t.encoded_frames.fetch_add(1, Ordering::Relaxed);
        t.encode_us_total.fetch_add(1234, Ordering::Relaxed);
        assert_eq!(t.queue_depth.load(Ordering::Relaxed), 1);
        assert_eq!(t.encoded_frames.load(Ordering::Relaxed), 1);
        assert_eq!(t.encode_us_total.load(Ordering::Relaxed), 1234);
        assert!(!t.failed.load(Ordering::Relaxed));
    }
}
