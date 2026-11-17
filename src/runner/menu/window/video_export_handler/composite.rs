//! 单帧合成 + 预览（键盘/标尺合成、预览、缓冲返回给入队方）。
//!
//! `write_frame` 已移至独立 ffmpeg 写线程（见 `encoder_thread.rs`），本模块
//! 只负责把 GPU 读回帧合成成最终帧数据并生成预览，随后返回给导出线程入队。

use std::time::{Duration, Instant};

use tokio::sync::mpsc::UnboundedSender;

use super::super::video_export;
use super::frame::{FrameParams, FrameStageStats};

/// 单帧合成输入（不含编码器：编码在独立写线程完成）。
pub(super) struct CompositeFrameInput<'a> {
    pub data: Vec<u8>,
    pub params: FrameParams,
    pub preview_tx: &'a UnboundedSender<(Vec<u8>, u32, u32)>,
    pub last_preview_time: &'a mut Instant,
    pub preview_sent: &'a mut bool,
    pub width: u32,
    pub height: u32,
    pub keyboard_pixels: &'a [u8],
    pub kb_w: u32,
    pub kb_h: u32,
    /// 128 键 RGBA 键色（渲染线程 GPU 活跃键内核推导，随帧同序送达）。
    pub key_colors: &'a [u8; 1024],
}

/// 单帧合成：键盘贴图 + 标尺数字 + 预览帧生成。
///
/// 返回 `(合成帧, stats)`；`None` 表示读回为空应跳过入队（与旧路径丢弃该帧等价）。
/// 瀑布流/MIDITrain/计数器模式传 `FrameParams::default()` + 空键盘贴图即可跳过合成。
pub(super) fn composite_frame(
    input: CompositeFrameInput<'_>,
) -> (Option<Vec<u8>>, FrameStageStats) {
    let CompositeFrameInput {
        mut data,
        params,
        preview_tx,
        last_preview_time,
        preview_sent,
        width,
        height,
        keyboard_pixels,
        kb_w,
        kb_h,
        key_colors,
    } = input;
    let mut stats = FrameStageStats::default();
    let FrameParams {
        scroll_x: sx,
        zoom_x: zx,
        keyboard_width: kw,
        ppq: ppq_val,
    } = params;

    if data.is_empty() {
        tracing::warn!("帧读回为空，跳过合成与入队");
        return (None, stats);
    }

    let t0 = Instant::now();
    if !keyboard_pixels.is_empty() {
        video_export::composite_keyboard(
            &mut data,
            width,
            height,
            keyboard_pixels,
            kb_w,
            kb_h,
            key_colors,
        );
        video_export::composite_ruler_numbers(&mut data, width, height, sx, zx, kw, ppq_val);
    }
    stats.composite_us = t0.elapsed().as_micros() as u64;

    // 预览帧：在 data 移交给入队方之前生成。
    // 第一帧立即发送，让预览界面尽快有内容；后续按 200ms 节流。
    // 使用 downscale_bgra_to_rgba 合并 BGRA→RGBA 交换与缩小为单次遍历，
    // 避免全帧 clone（~8MB@1080p）以节省内存带宽。
    if !*preview_sent || last_preview_time.elapsed() >= Duration::from_millis(200) {
        let t0 = Instant::now();
        // GPU 读回是 BGRA 格式，但 image::Handle::from_rgba 需要 RGBA
        const PREVIEW_MAX_W: u32 = 480;
        let (small_data, small_w, small_h) = if width > PREVIEW_MAX_W {
            let scale = PREVIEW_MAX_W as f64 / width as f64;
            let tw = PREVIEW_MAX_W;
            let th = (height as f64 * scale).round() as u32;
            // 单次分配 + 缩放 + BGR 交换，零额外 clone
            super::super::downscale_bgra_to_rgba(&data, width, height, tw, th)
        } else {
            // 不需要缩小：clone 并在 clone 上做 BGR 交换
            let mut preview_data = data.clone();
            for pixel in preview_data.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
            }
            (preview_data, width, height)
        };

        if preview_tx.send((small_data, small_w, small_h)).is_err() {
            tracing::warn!("视频导出: 预览帧发送失败，接收端已关闭");
        }
        *last_preview_time = Instant::now();
        *preview_sent = true;
        stats.preview_us = t0.elapsed().as_micros() as u64;
    }

    (Some(data), stats)
}
