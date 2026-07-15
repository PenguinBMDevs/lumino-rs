#![allow(deprecated)]
mod arrangement_instances;
mod arrangement_renderer;
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

mod swappable_buffer;

pub use arrangement_instances::{
    ARRANGEMENT_PALETTE, ArrangementSceneParams, ArrangementViewColors, ArrangementViewport,
    build_arrangement_all, collect_arrangement_instances,
};
pub use arrangement_renderer::{
    ArrangementNoteInstance, ArrangementRenderer, ArrangementUniform, colors,
};
pub use cc_bar_renderer::{
    CcBarColors, CcBarData, CcBarInstance, CcBarRenderer, CcBarViewParams, CcBarViewportUniform,
    build_cc_bar_instances,
};
pub use context::{Context, ContextError, Result};
pub use gpu_note_buffer::{GpuNoteBuffer, NoteEvent};
pub use grid::{GridViewParams, generate_grid_instances, generate_ruler_instances, is_black_key};
pub use grid_renderer::{GridLineInstance, GridPrepareParams, GridRenderer};
pub use keyboard_renderer::renderer::KeyboardPrepareParams;
pub use keyboard_renderer::{KeyInstance, KeyboardRenderer, KeyboardViewportUniform};
/// 洋葱皮音符类型（从 lumino-onion-skin 重导出）
pub use lumino_onion_skin::OnionSkinNote;
/// 高精度洋葱皮贴图渲染器（从 lumino-onion-skin-hires 重导出）
pub use lumino_onion_skin_hires::{
    CacheMeta, GenerateError, GroupTile, HiResConfig, HiResProgressCallback, HiResRenderMode,
    HiResRenderer, HiResUniform, TRACKS_PER_GROUP, TileCoord, TrackTile, compute_midi_hash,
    generate_all_tiles, generate_track_tile, merge_group_tiles, read_track_tile_cache,
};
pub use note_renderer::{
    CameraParams, CameraUniform, CullUniform, NoteInstance, NoteRenderer, OnionBgTileRef,
    RenderUniform, pack_color, unpack_color,
};
pub use ruler_renderer::{
    RulerPrepareParams, RulerRenderer, RulerTickInstance, RulerViewportUniform,
};
pub use swappable_buffer::{AtomicSwappableBuffer, MpscQueue, RenderData, SwappableBuffer};

pub use render_thread::RenderParams;
