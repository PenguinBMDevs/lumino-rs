//! 贴图瀑布流视频导出贴图上传

use crate::texture_waterfall::config::TextureWaterfallConfig;
use crate::texture_waterfall::gpu_ctx::WaterfallGpuCtx;
use crate::texture_waterfall::meta::WaterfallMeta;
use crate::texture_waterfall::renderer::TextureWaterfallRenderer;
use crate::texture_waterfall::types::WaterfallGroupTile;

/// 视频导出贴图上传参数
pub struct WaterfallUploadTileParams {
    /// 整合组贴图列表
    pub tiles: Vec<WaterfallGroupTile>,
    /// 贴图瀑布流配置
    pub config: TextureWaterfallConfig,
    /// 音轨总数
    pub track_count: u16,
    /// 键位数量（128 或 256）
    pub key_count: u16,
    /// 全曲总 tick
    pub total_ticks: u32,
    /// MIDI ppq
    pub ppq: u16,
}

// ── 视频导出贴图上传 ──────────────────────────────────

/// 上传视频导出预生成的贴图，并初始化渲染器与元数据
pub fn upload_waterfall_video_tiles(
    gpu: &WaterfallGpuCtx<'_>,
    renderer: &mut Option<TextureWaterfallRenderer>,
    meta: &mut Option<WaterfallMeta>,
    renderer_config: &mut Option<TextureWaterfallConfig>,
    params: WaterfallUploadTileParams,
) {
    let mut new_renderer =
        TextureWaterfallRenderer::new(gpu.device, params.config.clone(), gpu.texture_format);
    for tile in params.tiles {
        new_renderer.upload_tile(
            gpu.device,
            gpu.queue,
            tile.coord,
            &tile.pixels,
            tile.width,
            tile.height,
        );
    }

    let time_groups = params
        .config
        .time_group_count(params.total_ticks, params.ppq);
    let ticks_per_group = params.config.ticks_per_group(params.ppq);
    let track_groups = params.config.track_group_count(params.track_count);

    tracing::info!(
        "视频导出贴图瀑布流上传完成: {} 张, track_groups={}, time_groups={}",
        new_renderer.tile_count(),
        track_groups,
        time_groups
    );

    *renderer = Some(new_renderer);
    *renderer_config = Some(params.config);
    *meta = Some(WaterfallMeta {
        track_count: params.track_count,
        track_groups,
        key_count: params.key_count,
        time_groups,
        ticks_per_group,
    });
}
