pub mod automation;
pub mod editor_state;
pub mod editor_transform;
pub mod error;
pub mod font_scanner;
pub mod history;
pub mod i18n;
pub mod midi_types;
pub mod note;
pub mod note_store;
pub mod palette;
pub mod pattern;
pub mod smooth_scroll;
pub mod spatial_index;
pub mod storage;
pub mod types;
pub mod view_state;

pub use automation::{
    AutomationEdit, AutomationEvent, AutomationLane, AutomationTarget, SegmentShape,
};
pub use editor_state::{
    CanvasState, DEFAULT_BPM, DEFAULT_PREVIEW_VELOCITY, DragState, EditState, EditorData,
    EditorState, GLUE_PROXIMITY_THRESHOLD, HitType, InteractionState, SELECTION_BOX_EDGE_THRESHOLD,
    SelectionHitType,
};
pub use editor_transform::EditorTransform;
pub use error::{CoreError, Result};
pub use font_scanner::{FontInfo, get_cached_fonts, prewarm_font_cache, scan_system_fonts};
pub use history::{EditorSnapshot, History, HistoryEntry, MoveOp, OpKind, OperationEntry};
pub use midi_types::{
    BendDisplay, BendPoint, CC_CONTROLLER_NAMES, CcData, CcDisplay, CcPoint, EditMode,
    PITCH_BEND_CENTER, TempoPoint, VelocityPoint,
};
pub use note::Note;
pub use note_store::{BitSet, NoteMut, NoteStore, NoteView};
pub use pattern::Pattern;
pub use smooth_scroll::SmoothScrollAnimation;
pub use spatial_index::{NoteRef, NoteSpatialIndex};
pub use types::{AudioAction, DotType, NotePrecision, Tool};
pub use view_state::ViewState;
