#![allow(deprecated)]
mod arrangement_renderer;
pub mod constants;
mod context;
mod gpu_note_buffer;
mod grid_renderer;
mod keyboard_renderer;
mod note_renderer;
mod onion_renderer;
mod ruler_renderer;

#[cfg(feature = "unstable-swappable-buffer")]
mod swappable_buffer;

pub use arrangement_renderer::{ArrangementNoteInstance, ArrangementRenderer, ArrangementUniform, colors};
pub use context::{Context, ContextError, Result};
pub use gpu_note_buffer::{GpuNoteBuffer, NoteEvent};
pub use grid_renderer::{GridLineInstance, GridRenderer};
pub use keyboard_renderer::{KeyInstance, KeyboardRenderer, KeyboardViewportUniform};
pub use note_renderer::{
    CameraParams, CameraUniform, CullUniform, NoteInstance, NoteRenderer, OnionBgTileRef,
    RenderUniform, pack_color, unpack_color,
};
pub use onion_renderer::{
    OnionNote, OnionRenderer, OnionTrackColors, OnionTrackMask, OnionViewportUniform, TrackColor,
    convert_onion_colors,
};
pub use ruler_renderer::{RulerRenderer, RulerTickInstance, RulerViewportUniform};

#[cfg(feature = "unstable-swappable-buffer")]
pub use swappable_buffer::{AtomicSwappableBuffer, MpscQueue, RenderData, SwappableBuffer};
