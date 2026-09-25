use super::*;

use super::super::super::Renderers;
use super::super::onion_segments::{
    apply_onion_track_delta, apply_onion_track_layout, apply_onion_track_remove_ranges,
};

/// drain 贴图瀑布流流式上传通道：逐块 streaming_append 到 GPU。
///
/// UI 线程分块构建 NoteInstance（每块 ≤ 800 万实例 = 128 MB），通过 sync_channel(3) 传输。
/// 消息协议（事件级增量 2026-08-05）：
/// - `BeginSession`：全量会话开始（清空段表 + 重置计数，空文档会话也生效）
/// - `Chunk{track_id}`：全量会话数据块，按到达顺序构建段表（同轨续写、异轨新段）
/// - `Done`：全量会话完成 → finish_streaming_upload 更新 cull info（段表保留）
/// - `TrackLayout{track_count}`：文档轨数变化 → 增量增删段表（不重建既有轨）
/// - `TrackDelta`：单音轨增量替换 → 等长 write_segment / 变长 GPU 搬移后续段
/// - `TrackRemoveRanges`：单音轨区间级删除 → 段内删除 + 段表长度/偏移联动
/// - `SetViewState`：切轨/静音零重传，只更新 ViewState uniform，GPU 数据不动
/// - `PreviewInstances`：预览音符（Drawing/hover/i2m），独立预览渲染器整体替换
///
/// 性能：消除旧方案 RenderParams.onion_skin_instances 全量 Vec 的 9.6 GB CPU 峰值；
/// 黑乐谱编辑非主音轨时不再全量重传。
pub(super) fn drain_onion_skin_stream(
    ctx: &RenderContext,
    renderers: &mut Renderers,
    onion_segments: &mut Vec<OnionSegment>,
    onion_skin_streaming_in_progress: &mut bool,
    current_track_encoded: &mut u32,
    rx: &std::sync::mpsc::Receiver<crate::OnionSkinStreamMsg>,
) {
    loop {
        match rx.try_recv() {
            Ok(crate::OnionSkinStreamMsg::BeginSession) => {
                // 全量会话开始：清空段表 + 重置计数（无 Chunk 的空文档会话也清表）
                renderers.onion_skin.begin_streaming_upload();
                onion_segments.clear();
                *onion_skin_streaming_in_progress = true;
            }
            Ok(crate::OnionSkinStreamMsg::Done) => {
                // 全量会话结束
                if *onion_skin_streaming_in_progress {
                    renderers
                        .onion_skin
                        .finish_streaming_upload(&ctx.device, &ctx.queue);
                    *onion_skin_streaming_in_progress = false;
                    // 全量会话结束 → 常驻数据换代，派生索引（全局桶）必须重建。
                    renderers.onion_epoch = renderers.onion_epoch.wrapping_add(1);
                    let total_gpu_mb =
                        lumino_diagnostics::memtrace::Snapshot::capture().total_with_gpu_mb();
                    tracing::debug!(
                        "OnionSkin: 全量上传完成 instance_count={} instance_buf={}MB visible_index_buf={}MB total_gpu={:.1}MB",
                        renderers.onion_skin.gpu_instance_count(),
                        renderers.onion_skin.instance_buffer_size() / 1024 / 1024,
                        renderers.onion_skin.visible_buffer_size() / 1024 / 1024,
                        total_gpu_mb
                    );
                }
                // 段表必须保留：后续 `TrackDelta` 与 `process_main_track_events`
                // 依赖它定位音轨段。
                break;
            }
            Ok(crate::OnionSkinStreamMsg::Reserve { total }) => {
                // 预分配容量：消除流式 append 的 2× 倍增余量
                // （2.9 亿音符容量从 ~8.6GB 收到 ~4.6GB）
                if total > renderers.onion_skin.gpu_capacity() {
                    renderers.onion_skin.grow_gpu(total);
                    tracing::debug!("OnionSkin: 预分配容量 {} 实例（消除倍增余量）", total);
                }
            }
            Ok(crate::OnionSkinStreamMsg::Chunk {
                track_id,
                instances,
            }) => {
                // 防御性兜底：未收到 BeginSession 就直接来块（旧协议/异常序）时补一次
                if !*onion_skin_streaming_in_progress {
                    renderers.onion_skin.begin_streaming_upload();
                    onion_segments.clear();
                    *onion_skin_streaming_in_progress = true;
                }
                // 段表：同轨续写 len，异轨新开段（offset = 追加前实例数）
                let count_before_append = renderers.onion_skin.gpu_instance_count();
                if let Some(last) = onion_segments.last_mut() {
                    if last.track_id == track_id {
                        last.len += instances.len();
                    } else {
                        onion_segments.push(OnionSegment {
                            track_id,
                            offset: count_before_append,
                            len: instances.len(),
                        });
                    }
                } else {
                    onion_segments.push(OnionSegment {
                        track_id,
                        offset: count_before_append,
                        len: instances.len(),
                    });
                }
                renderers.onion_skin.streaming_append(&instances);
            }
            Ok(crate::OnionSkinStreamMsg::TrackLayout { track_count }) => {
                if *onion_skin_streaming_in_progress {
                    // 全量会话中布局由 `Chunk` 流构建，跳过（防御性）
                    tracing::warn!(
                        "OnionSkin: TrackLayout({}) 与全量流式会话交错，跳过（会话完成后由 UI 补发）",
                        track_count
                    );
                } else {
                    apply_onion_track_layout(
                        renderers,
                        onion_segments,
                        track_count,
                        &ctx.device,
                        &ctx.queue,
                    );
                }
            }
            Ok(crate::OnionSkinStreamMsg::TrackDelta { track_id, parts }) => {
                if *onion_skin_streaming_in_progress {
                    // UI 不应在全量会话中夹带增量（防御性：状态不一致时跳过）
                    tracing::warn!(
                        "OnionSkin: TrackDelta(track={}) 与全量流式会话交错，跳过该增量",
                        track_id
                    );
                } else {
                    apply_onion_track_delta(
                        renderers,
                        onion_segments,
                        track_id,
                        &parts,
                        &ctx.device,
                        &ctx.queue,
                    );
                }
            }
            Ok(crate::OnionSkinStreamMsg::TrackRemoveRanges { track_id, ranges }) => {
                if *onion_skin_streaming_in_progress {
                    tracing::warn!(
                        "OnionSkin: TrackRemoveRanges(track={}) 与全量流式会话交错，跳过该增量",
                        track_id
                    );
                } else {
                    apply_onion_track_remove_ranges(
                        renderers,
                        onion_segments,
                        track_id,
                        &ranges,
                        &ctx.device,
                        &ctx.queue,
                    );
                }
            }
            Ok(crate::OnionSkinStreamMsg::SetViewState {
                current_track,
                muted_tracks,
            }) => {
                // 切轨/静音零重传：只更新 ViewState uniform，GPU 数据不动
                *current_track_encoded = current_track;
                renderers
                    .onion_skin
                    .set_view_state(&ctx.queue, current_track, &muted_tracks);
            }
            Ok(crate::OnionSkinStreamMsg::PreviewInstances(instances)) => {
                // 预览音符（Drawing/hover/i2m）：独立预览渲染器整体替换
                renderers
                    .note
                    .upload_instances(&instances, &ctx.device, &ctx.queue);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => break,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                // UI 线程关闭 channel（shutdown），停止 drain
                break;
            }
        }
    }
}
