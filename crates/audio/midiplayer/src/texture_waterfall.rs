//! 贴图瀑布流
//!
//! 在 P1 全曲低精度底图基础上，提供按时间分组、音轨分组的贴图瀑布流，
//! 用于视口放大时显示清晰概览。
//!
//! ## 贴图规格
//! - 格式：`RGBA8_UNORM`
//! - 宽度：默认 1920 像素（覆盖 4 小节，可配）
//! - 高度：128（128key 模式）或 256（256key 模式），1 key = 1 px
//! - 采样：Nearest（避免缩放模糊）
//!
//! ## 分组
//! - 时间组：每 `measures_per_group` 小节（默认 4）为一张贴图
//! - 音轨组：每 8 轨为一组，rayon 并行生成
//! - 整合组贴图：8 轨顺序叠加（后轨覆盖前轨重叠区），规格与单音轨贴图相同
//!
//! ## 缓存
//! 单音轨贴图落盘 `.lmocache`（zstd level 3 压缩 RGBA8），
//! 位于系统 temp 目录或用户自定义目录，按 MIDI 内容哈希分桶。
//!
//! ## 视口驱动
//! 视口可见范围 / 全曲 < 阈值（默认 40%）时启用贴图瀑布流，
//! 按可见时间组上传 GPU，不可见贴图 LRU 淘汰。

mod cache;
mod command;
mod config;
mod generate;
mod gpu_ctx;
mod meta;
mod note;
mod renderer;
mod runner;
mod scheduler;
mod stream;
mod track_params;
mod types;
mod viewport;

pub use cache::{
    WaterfallCacheError, WaterfallCacheMeta, clear_all_waterfall_cache, clear_midi_waterfall_cache,
    compute_waterfall_cache_hash, read_waterfall_track_tile_cache, waterfall_cache_path,
    write_waterfall_track_tile_cache,
};
pub use command::WaterfallCommand;
pub use config::{
    TextureWaterfallConfig, TextureWaterfallRenderMode, WATERFALL_TRACKS_PER_GROUP,
    WaterfallConfigError,
};
pub use generate::{
    generate_waterfall_track_tile, merge_waterfall_group_tiles, merge_waterfall_track_tile_into,
};
pub use gpu_ctx::WaterfallGpuCtx;
pub use meta::WaterfallMeta;
pub use note::WaterfallNote;
pub use renderer::{TextureWaterfallRenderer, TextureWaterfallUniform};
pub use runner::{
    WaterfallGenerateContext, WaterfallUploadTileParams, drain_waterfall_stream,
    ensure_renderer_for_config, handle_regenerate_waterfall_track, handle_waterfall_dirty_overlay,
    handle_waterfall_dispose, handle_waterfall_generate, push_waterfall_progress,
    update_waterfall_viewport, upload_waterfall_video_tiles,
};
pub use scheduler::{
    TextureWaterfallProgressCallback, WaterfallGenContext, WaterfallGenerateError,
    generate_waterfall_tiles, generate_waterfall_tiles_streaming,
};
pub use stream::WaterfallStreamMsg;
pub use track_params::WaterfallTrackParams;
pub use types::{
    WaterfallDirtyKind, WaterfallDirtyRegion, WaterfallGroupTile, WaterfallTileCoord,
    WaterfallTrackTile,
};
pub use viewport::WaterfallViewportParams;
