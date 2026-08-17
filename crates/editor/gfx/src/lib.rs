#![allow(deprecated)]
mod arrangement_instances;
mod arrangement_renderer;
pub mod cache;
mod cc_bar_renderer;
pub mod constants;
mod context;
mod gpu_note_buffer;
mod gpu_resource_tracker;
pub mod grid;
mod grid_renderer;
mod keyboard_renderer;
mod note_renderer;
pub mod render_thread;
mod ruler_renderer;
// mod velocity_line_renderer; // 已弃用 — 改用 CcBarRenderer

pub mod automation;
pub mod miditrail_renderer;
pub mod waterfall_renderer;

mod swappable_buffer;

pub use arrangement_instances::{
    ArrangementSceneParams, ArrangementViewColors, ArrangementViewport, build_arrangement_all,
    collect_arrangement_instances,
};
pub use arrangement_renderer::{
    ArrangementNoteInstance, ArrangementRenderer, ArrangementUniform, colors,
};
pub use cc_bar_renderer::{
    CcBarColors, CcBarData, CcBarInstance, CcBarRenderer, CcBarViewParams, CcBarViewportUniform,
    build_cc_bar_instances,
};
pub use context::{Context, ContextError, Result};
pub use gpu_note_buffer::{GpuNoteBuffer, NoteEvent, OnionSkinStreamMsg};
pub use grid::{generate_ruler_instances, is_black_key};
pub use grid_renderer::{GridLineInstance, GridPrepareParams, GridRenderer};
pub use keyboard_renderer::renderer::KeyboardPrepareParams;
pub use keyboard_renderer::{KeyInstance, KeyboardRenderer, KeyboardViewportUniform};
/// 贴图瀑布流音符类型（从 lumino-midiplayer 重导出）
pub use lumino_midiplayer::texture_waterfall::WaterfallNote;
/// 贴图瀑布流渲染器（从 lumino-midiplayer 重导出）
pub use lumino_midiplayer::texture_waterfall::{
    TextureWaterfallConfig, TextureWaterfallProgressCallback, TextureWaterfallRenderMode,
    TextureWaterfallRenderer, TextureWaterfallUniform, WATERFALL_TRACKS_PER_GROUP,
    WaterfallCacheMeta, WaterfallCommand, WaterfallGenContext, WaterfallGenerateError,
    WaterfallGpuCtx, WaterfallGroupTile, WaterfallMeta, WaterfallStreamMsg, WaterfallTileCoord,
    WaterfallTrackParams, WaterfallTrackTile, WaterfallViewportParams,
    compute_waterfall_cache_hash, generate_waterfall_tiles, generate_waterfall_tiles_streaming,
    generate_waterfall_track_tile, merge_waterfall_group_tiles, merge_waterfall_track_tile_into,
    read_waterfall_track_tile_cache,
};
pub use note_renderer::{
    CameraParams, CameraUniform, CullUniform, NoteInstance, NoteRenderer, PREVIEW_BORDER_SENTINEL,
    RenderUniform, ViewState, calculate_border_width, pack_key_color, unpack_key_color,
};
pub use ruler_renderer::{
    RulerPrepareParams, RulerRenderer, RulerTickInstance, RulerViewportUniform,
};
pub use swappable_buffer::{AtomicSwappableBuffer, MpscQueue, RenderData, SwappableBuffer};

pub use miditrail_renderer::{
    MiditrailNoteGpu, MiditrailRenderer, MiditrailUniformGpu, pack_color,
};
pub use render_thread::{RenderParams, WgpuRenderThread};
pub use waterfall_renderer::{WaterfallNoteGpu, WaterfallRenderer, WaterfallUniformGpu};
/// 重导出 wgpu 纹理格式，供 UI 层匹配视频导出像素格式
pub use wgpu::TextureFormat;

#[cfg(test)]
mod test_minimal;
