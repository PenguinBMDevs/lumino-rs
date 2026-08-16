use std::sync::{Arc, Mutex};

use crate::{TextureWaterfallRenderer, WaterfallTileCoord};

use super::super::context::RenderContext;
use super::super::types::HiResStreamMsg;
use super::common::push_onion_progress;

// ── 流式接收后台生成的贴图瀑布流并上传到 GPU ──────────────

/// 每帧循环 `try_recv`，收到已合并像素立即 `upload_tile`（GPU DMA，非阻塞）；
/// 收到 `Finished` 后 flush DMA 并推送完成进度。无更多消息即退出本帧接收。
pub(crate) fn drain_hires_stream(
    hires_result_rx: &std::sync::mpsc::Receiver<HiResStreamMsg>,
    ctx: &RenderContext,
    hires_renderer: &mut Option<TextureWaterfallRenderer>,
    onion_progress: &Arc<Mutex<Vec<(String, f32)>>>,
) {
    loop {
        match hires_result_rx.try_recv() {
            Ok(HiResStreamMsg::TimeGroupMerged {
                track_group,
                time_group,
                pixels,
                width,
                height,
            }) => {
                if let Some(renderer) = hires_renderer {
                    let coord = WaterfallTileCoord::new(track_group, time_group);
                    renderer.upload_tile(&ctx.device, &ctx.queue, coord, &pixels, width, height);
                }
            }
            Ok(HiResStreamMsg::ClearDirtyOverlay(track_group)) => {
                if let Some(renderer) = hires_renderer {
                    renderer.clear_dirty_overlays(track_group);
                }
            }
            Ok(HiResStreamMsg::Finished) => {
                if hires_renderer.is_some() {
                    let flush =
                        ctx.device
                            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: Some("hires_stream_flush"),
                            });
                    ctx.queue.submit(std::iter::once(flush.finish()));
                }
                push_onion_progress(onion_progress, "贴图瀑布流流式生成+上传完成", 1.0);
            }
            Err(_) => break,
        }
    }
}
