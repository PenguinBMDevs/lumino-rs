#![allow(deprecated)]
mod arrangement_instances;
mod arrangement_renderer;
mod cc_bar_renderer;
pub mod constants;
mod context;
mod gpu_note_buffer;
pub mod grid;
mod grid_renderer;
mod keyboard_renderer;
mod note_renderer;
mod onion_renderer;
mod onion_skin_bucket;
pub mod render_thread;
mod ruler_renderer;
// mod velocity_line_renderer; // 已弃用 — 改用 CcBarRenderer

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
pub use note_renderer::{
    CameraParams, CameraUniform, CullUniform, NoteInstance, NoteRenderer, OnionBgTileRef,
    RenderUniform, pack_color, unpack_color,
};
pub use onion_renderer::{OnionNote, OnionRenderer, OnionViewportUniform};
pub use onion_skin_bucket::{OnionNoteList, build_list_from_notes};
pub use ruler_renderer::{
    RulerPrepareParams, RulerRenderer, RulerTickInstance, RulerViewportUniform,
};
pub use swappable_buffer::{AtomicSwappableBuffer, MpscQueue, RenderData, SwappableBuffer};
