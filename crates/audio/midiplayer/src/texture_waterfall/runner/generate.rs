//! 贴图瀑布流后台流式生成与资源释放

use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};

use super::common::{ensure_renderer_for_config, push_waterfall_progress};
use crate::texture_waterfall::config::TextureWaterfallConfig;
use crate::texture_waterfall::gpu_ctx::WaterfallGpuCtx;
use crate::texture_waterfall::meta::WaterfallMeta;
use crate::texture_waterfall::note::WaterfallNote;
use crate::texture_waterfall::renderer::TextureWaterfallRenderer;
use crate::texture_waterfall::scheduler::{
    TextureWaterfallProgressCallback, WaterfallGenContext, generate_waterfall_tiles_streaming,
};
use crate::texture_waterfall::stream::WaterfallStreamMsg;
use crate::texture_waterfall::types::WaterfallGroupTile;

/// 贴图瀑布流后台生成上下文。
///
/// 聚合 `handle_waterfall_generate` 中后台线程生成贴图所需的全部参数。
/// owned 字段在生成前被移入后台线程；引用字段在生成前完成初始化。
pub struct WaterfallGenerateContext<'a> {
    /// GPU 基础设施引用
    pub gpu: &'a WaterfallGpuCtx<'a>,
    /// 每轨音符列表
    pub notes: Vec<Vec<WaterfallNote>>,
    /// MIDI ppq
    pub ppq: u16,
    /// 键位数量
    pub key_count: u16,
    /// 全曲总 tick
    pub total_ticks: u32,
    /// 贴图瀑布流配置
    pub config: TextureWaterfallConfig,
    /// MIDI 内容哈希
    pub midi_hash: String,
    /// 结果发送通道（流式贴图消息）
    pub result_tx: &'a SyncSender<WaterfallStreamMsg>,
    /// 进度缓冲
    pub progress: &'a Arc<Mutex<Vec<(String, f32)>>>,
    /// 贴图瀑布流渲染器
    pub renderer: &'a mut Option<TextureWaterfallRenderer>,
    /// 贴图瀑布流元数据
    pub meta: &'a mut Option<WaterfallMeta>,
    /// 贴图瀑布流渲染器配置
    pub renderer_config: &'a mut Option<TextureWaterfallConfig>,
}

// ── 初始化与全轨后台流式生成 ──────────────────────────────

/// 初始化渲染器并设置元数据
fn setup_generate_context(context: &mut WaterfallGenerateContext<'_>) {
    ensure_renderer_for_config(
        context.gpu,
        context.renderer,
        context.renderer_config,
        &context.config,
    );

    let track_count = context.notes.len() as u16;
    let time_groups = context
        .config
        .time_group_count(context.total_ticks, context.ppq);
    let ticks_per_group = context.config.ticks_per_group(context.ppq);
    *context.meta = Some(WaterfallMeta {
        track_count,
        track_groups: 1,
        key_count: context.key_count,
        time_groups,
        ticks_per_group,
    });
}

/// 启动后台线程，流式生成全轨合并贴图并通过通道发送
#[allow(clippy::too_many_arguments)] // 生成参数完整显式传递，避免结构体间接引用
fn spawn_streaming_generation(
    progress_buf: Arc<Mutex<Vec<(String, f32)>>>,
    result_tx: &SyncSender<WaterfallStreamMsg>,
    tile_width: u32,
    tile_height: u32,
    mut notes: Vec<Vec<WaterfallNote>>,
    config: TextureWaterfallConfig,
    midi_hash: String,
    ppq: u16,
    key_count: u16,
    total_ticks: u32,
) {
    let tx = Arc::new(Mutex::new(result_tx.clone()));
    let cb: TextureWaterfallProgressCallback = Arc::new(move |msg, pct| {
        if let Ok(mut buf) = progress_buf.lock() {
            buf.push((msg.to_string(), pct.clamp(0.0, 1.0)));
        }
    });

    std::thread::spawn(move || {
        let time_group_cb = {
            let tx = Arc::clone(&tx);
            let (tw, th) = (tile_width, tile_height);
            move |time_group: u32, tile: WaterfallGroupTile| {
                if let Ok(guard) = tx.lock() {
                    let _ = guard.send(WaterfallStreamMsg::TimeGroupMerged {
                        track_group: tile.coord.track_group,
                        time_group,
                        pixels: tile.pixels,
                        width: tw,
                        height: th,
                    });
                }
            }
        };

        let stream_ctx = WaterfallGenContext {
            config: &config,
            ppq,
            key_count,
            total_ticks,
            midi_hash: &midi_hash,
        };
        generate_waterfall_tiles_streaming(&mut notes, &stream_ctx, Some(cb), &time_group_cb);

        if let Ok(guard) = tx.lock() {
            let _ = guard.send(WaterfallStreamMsg::Finished);
        }
    });
}

/// 处理 `WaterfallCommand::Generate`：启动后台流式生成
pub fn handle_waterfall_generate(mut context: WaterfallGenerateContext<'_>) {
    setup_generate_context(&mut context);
    push_waterfall_progress(context.progress, "正在后台生成贴图瀑布流\u{2026}", 0.0);

    spawn_streaming_generation(
        Arc::clone(context.progress),
        context.result_tx,
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

// ── 释放贴图瀑布流资源 ──────────────────────────────────

/// 处理 `WaterfallCommand::Dispose`：释放渲染器、元数据与配置
pub fn handle_waterfall_dispose(
    renderer: &mut Option<TextureWaterfallRenderer>,
    meta: &mut Option<WaterfallMeta>,
    renderer_config: &mut Option<TextureWaterfallConfig>,
    progress: &Arc<Mutex<Vec<(String, f32)>>>,
) {
    *renderer = None;
    *meta = None;
    *renderer_config = None;
    push_waterfall_progress(progress, "贴图瀑布流资源已释放", 1.0);
}
