//! 贴图瀑布流流式接收与 GPU 上传

use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};

use super::common::push_waterfall_progress;
use crate::texture_waterfall::gpu_ctx::WaterfallGpuCtx;
use crate::texture_waterfall::renderer::TextureWaterfallRenderer;
use crate::texture_waterfall::stream::WaterfallStreamMsg;
use crate::texture_waterfall::types::WaterfallTileCoord;

// ── 流式接收后台生成的贴图并上传到 GPU ──────────────

/// 每帧循环 `try_recv`，收到已合并像素立即 `upload_tile`（GPU DMA，非阻塞）；
/// 收到 `Finished` 后 flush DMA 并推送完成进度。无更多消息即退出本帧接收。
pub fn drain_waterfall_stream(
    result_rx: &Receiver<WaterfallStreamMsg>,
    gpu: &WaterfallGpuCtx<'_>,
    renderer: &mut Option<TextureWaterfallRenderer>,
    progress: &Arc<Mutex<Vec<(String, f32)>>>,
) {
    loop {
        match result_rx.try_recv() {
            Ok(WaterfallStreamMsg::TimeGroupMerged {
                track_group,
                time_group,
                pixels,
                width,
                height,
            }) => {
                if let Some(renderer) = renderer {
                    let coord = WaterfallTileCoord::new(track_group, time_group);
                    renderer.upload_tile(gpu.device, gpu.queue, coord, &pixels, width, height);
                }
            }
            Ok(WaterfallStreamMsg::ClearDirtyOverlay(track_group)) => {
                if let Some(renderer) = renderer {
                    renderer.clear_dirty_overlays(track_group);
                }
            }
            Ok(WaterfallStreamMsg::Finished) => {
                if renderer.is_some() {
                    let flush =
                        gpu.device
                            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: Some("waterfall_stream_flush"),
                            });
                    gpu.queue.submit(std::iter::once(flush.finish()));
                }
                push_waterfall_progress(progress, "贴图瀑布流流式生成+上传完成", 1.0);
            }
            Err(_) => break,
        }
    }
}
