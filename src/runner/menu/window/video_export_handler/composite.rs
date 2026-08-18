//! 单帧合成 + 编码（键盘/标尺合成、预览、编码、缓冲归还）。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use lumino_export::video::FfmpegEncoder;
use tokio::sync::mpsc::UnboundedSender;

use super::super::video_export;
use super::commands::send_export_error;
use super::frame::{FrameParams, FrameStageStats};

/// 进度消息载荷：(文本, 进度 0..1, 总帧数, 平滑 FPS, 已用秒)
type ProgressMsg = (String, f64, u64, f64, f64);

/// 单帧处理：键盘贴图合成 + 标尺数字合成 + 取消检测 + 预览帧发送 + 编码 + 缓冲区归还。
///
/// 瀑布流/MIDITrail/计数器模式帧已由 GPU/CPU 完整渲染，调用时传
/// `FrameParams::default()` + 空键盘贴图即可跳过合成（与旧 `composite_waterfall_and_encode_frame` 等价）。
///
/// 返回 `(should_stop, stats)`：`should_stop` 为 true 表示应终止渲染循环（取消或出错）。
#[allow(clippy::too_many_arguments)]
pub(super) fn composite_and_encode_frame(
    mut data: Vec<u8>,
    params: FrameParams,
    encoder: &mut FfmpegEncoder,
    progress_tx: &UnboundedSender<ProgressMsg>,
    preview_tx: &UnboundedSender<(Vec<u8>, u32, u32)>,
    cancel_flag: &AtomicBool,
    last_preview_time: &mut Instant,
    preview_sent: &mut bool,
    width: u32,
    height: u32,
    keyboard_pixels: &[u8],
    kb_w: u32,
    kb_h: u32,
    recycle_tx: &Sender<Vec<u8>>,
) -> (bool, FrameStageStats) {
    let mut stats = FrameStageStats::default();
    let FrameParams {
        scroll_x: sx,
        zoom_x: zx,
        keyboard_width: kw,
        ppq: ppq_val,
        key_colors,
        ..
    } = params;

    if data.is_empty() {
        tracing::warn!("帧读回为空，跳过");
        return (false, stats);
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
            &key_colors,
        );
        video_export::composite_ruler_numbers(&mut data, width, height, sx, zx, kw, ppq_val);
    }
    stats.composite_us = t0.elapsed().as_micros() as u64;

    if cancel_flag.load(Ordering::Relaxed) {
        tracing::info!("视频导出：帧数据到达后检测到取消，正在收尾...");
        match encoder.write_frame(data) {
            Ok(frame) => {
                if recycle_tx.send(frame).is_err() {
                    tracing::warn!("取消收尾时帧缓冲区归还失败");
                }
            }
            Err(e) => {
                tracing::error!("取消收尾写入失败: {e}");
                send_export_error(progress_tx, format!("导出失败: {e}"));
            }
        }
        return (true, stats);
    }

    // 预览帧：在 write_frame（move data）之前生成。
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
            for pixel in preview_data.chunks_exact_mut(4) {
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

    let t0 = Instant::now();
    let encoded_frame = match encoder.write_frame(data) {
        Ok(buf) => buf,
        Err(e) => {
            tracing::error!("写入视频帧失败: {e}");
            send_export_error(progress_tx, format!("导出失败: {e}"));
            return (true, stats);
        }
    };
    stats.encode_us = t0.elapsed().as_micros() as u64;

    // 将已写入的帧缓冲区归还给渲染线程对象池复用
    if recycle_tx.send(encoded_frame).is_err() {
        tracing::warn!("帧缓冲区归还失败：回收通道已关闭");
    }

    (false, stats)
}
