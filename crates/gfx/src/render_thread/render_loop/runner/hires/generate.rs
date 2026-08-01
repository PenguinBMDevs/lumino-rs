use std::sync::{Arc, Mutex};

use crate::{GroupTile, HiResConfig, HiResProgressCallback, HiResRenderer};
use lumino_onion_skin_hires::StreamingGenContext;

use super::super::context::HiResGenerateContext;
use super::super::types::{HiResMeta, HiResStreamMsg};
use super::common::{ensure_renderer_for_config, push_onion_progress};

// ── 初始化与全轨后台流式生成 ──────────────────────────────

/// 初始化渲染器并设置元数据
fn setup_generate_context(context: &mut HiResGenerateContext<'_>) {
    ensure_renderer_for_config(
        context.ctx,
        context.hires_renderer,
        context.hires_config,
        &context.config,
    );

    let track_count = context.notes.len() as u16;
    let time_groups = context
        .config
        .time_group_count(context.total_ticks, context.ppq);
    let ticks_per_group = context.config.ticks_per_group(context.ppq);
    *context.hires_meta = Some(HiResMeta {
        track_count,
        track_groups: 1,
        key_count: context.key_count,
        time_groups,
        ticks_per_group,
    });
}

/// 启动后台线程，流式生成全轨合并贴图并通过通道发送
fn spawn_streaming_generation(
    progress_buf: Arc<Mutex<Vec<(String, f32)>>>,
    hires_result_tx: &std::sync::mpsc::SyncSender<HiResStreamMsg>,
    tile_width: u32,
    tile_height: u32,
    mut notes: Vec<Vec<lumino_onion_skin::OnionSkinNote>>,
    config: HiResConfig,
    midi_hash: String,
    ppq: u16,
    key_count: u16,
    total_ticks: u32,
) {
    let tx = Arc::new(Mutex::new(hires_result_tx.clone()));
    let cb: HiResProgressCallback = Arc::new(move |msg, pct| {
        if let Ok(mut buf) = progress_buf.lock() {
            buf.push((msg.to_string(), pct.clamp(0.0, 1.0)));
        }
    });

    std::thread::spawn(move || {
        let time_group_cb = {
            let tx = tx.clone();
            let (tw, th) = (tile_width, tile_height);
            move |time_group: u32, tile: GroupTile| {
                if let Ok(guard) = tx.lock() {
                    let _ = guard.send(HiResStreamMsg::TimeGroupMerged {
                        track_group: tile.coord.track_group,
                        time_group,
                        pixels: tile.pixels,
                        width: tw,
                        height: th,
                    });
                }
            }
        };

        let stream_ctx = StreamingGenContext {
            config: &config,
            ppq,
            key_count,
            total_ticks,
            midi_hash: &midi_hash,
        };
        lumino_onion_skin_hires::generate_all_tiles_streaming(
            &mut notes,
            &stream_ctx,
            Some(cb),
            &time_group_cb,
        );

        if let Ok(guard) = tx.lock() {
            let _ = guard.send(HiResStreamMsg::Finished);
        }
    });
}

/// 处理 GenerateHiResOnionSkin
pub(crate) fn handle_generate_hires(mut context: HiResGenerateContext<'_>) {
    setup_generate_context(&mut context);
    push_onion_progress(
        context.onion_progress,
        "正在后台生成高精度洋葱皮贴图\u{2026}",
        0.0,
    );

    spawn_streaming_generation(
        context.onion_progress.clone(),
        context.hires_result_tx,
        context.config.tile_width_px,
        context.key_count as u32,
        context.notes,
        context.config,
        context.midi_hash,
        context.ppq,
        context.key_count,
        context.total_ticks,
    );
}

// ── 释放高精度洋葱皮资源 ──────────────────────────────────

/// 处理 DisposeHiResOnionSkin
pub(crate) fn handle_dispose_hires(
    hires_renderer: &mut Option<HiResRenderer>,
    hires_meta: &mut Option<HiResMeta>,
    hires_config: &mut Option<HiResConfig>,
    onion_progress: &Arc<Mutex<Vec<(String, f32)>>>,
) {
    *hires_renderer = None;
    *hires_meta = None;
    *hires_config = None;
    push_onion_progress(onion_progress, "高精度洋葱皮资源已释放", 1.0);
}
