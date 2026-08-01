use crate::{GroupTile, HiResConfig, HiResRenderer};

use super::super::context::{HiResUploadContext, RenderContext, UploadHiResTileParams};
use super::super::types::HiResMeta;

// ── 视频导出高精度贴图上传 ──────────────────────────────────

/// 上传视频导出预生成的高精度贴图，并初始化渲染器与元数据
pub(crate) fn upload_hires_video_tiles(
    ctx: &RenderContext,
    hires_renderer: &mut Option<HiResRenderer>,
    hires_meta: &mut Option<HiResMeta>,
    hires_config: &mut Option<HiResConfig>,
    params: UploadHiResTileParams,
) {
    let mut renderer = HiResRenderer::new(&ctx.device, params.config.clone(), ctx.texture_format);
    for tile in params.tiles {
        renderer.upload_tile(
            &ctx.device,
            &ctx.queue,
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
        "视频导出 HiRes 贴图上传完成: {} 张, track_groups={}, time_groups={}",
        renderer.tile_count(),
        track_groups,
        time_groups
    );

    *hires_renderer = Some(renderer);
    *hires_config = Some(params.config);
    *hires_meta = Some(HiResMeta {
        track_count: params.track_count,
        track_groups,
        key_count: params.key_count,
        time_groups,
        ticks_per_group,
    });
}

/// 将 UploadHiResVideoTiles 命令中的字段打包并上传高精度贴图。
pub(crate) fn upload_hires_video_tiles_command(
    context: &mut HiResUploadContext<'_>,
    params: UploadHiResTileParams,
) {
    upload_hires_video_tiles(
        context.ctx,
        context.hires_renderer,
        context.hires_meta,
        context.hires_config,
        params,
    );
}
