pub mod arrange_selection;
pub mod automation;
pub mod batch_edit;
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
pub mod project;
pub mod smooth_scroll;
pub mod spatial_index;
pub mod storage;
pub mod types;
pub mod view_state;

/// 重新导出 `im` crate，便于上层组件直接引用 `im::Vector` 等持久化集合类型。
pub use im;

pub use arrange_selection::ArrangeSelection;
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
pub use project::{
    LoadedFileEntry, LoadedFormat, LuminoProject, TrackSlot, TrackVisibilitySer,
    archive::{ArchiveHeader, FileEntry, FileTable, build_archive, read_file_from_archive},
    data_formats::{LmctlData, LmnamesData, LmsigData, LmtempData},
    folder::FolderPaths,
    load::load_project,
    metadata::ProjectMetadata,
    save::{save_to_archive, save_to_folder},
    track::{LmtrackData, LmtrackHeader, TrackMeta},
};
pub use smooth_scroll::SmoothScrollAnimation;
pub use spatial_index::{NoteRef, NoteSpatialIndex};
pub use types::{AudioAction, DotType, NotePrecision, Tool};
pub use view_state::ViewState;
