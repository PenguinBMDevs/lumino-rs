pub mod editor_state;
pub mod error;
pub mod font_scanner;
pub mod history;
pub mod midi_types;
pub mod note;
pub mod pattern;
pub mod smooth_scroll;
pub mod spatial_index;
pub mod storage;
pub mod view_state;

pub use editor_state::{
    CanvasState, EditState, EditorData, EditorState, HitType, InteractionState,
    SelectionHitType,
};
pub use error::{CoreError, Result};
pub use font_scanner::{FontInfo, scan_system_fonts};
pub use history::{EditorSnapshot, History};
pub use midi_types::{
    BendDisplay, BendPoint, CcData, CcDisplay, CcPoint, EditMode, TempoPoint, VelocityPoint,
    CC_CONTROLLER_NAMES,
};
pub use note::Note;
pub use pattern::Pattern;
pub use smooth_scroll::SmoothScrollAnimation;
pub use spatial_index::{NoteRef, NoteSpatialIndex};
pub use view_state::ViewState;
