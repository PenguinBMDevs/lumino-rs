//! 音符核心模块
//!
//! 提供音符存储、历史记录、MIDI 类型、音符操作等核心功能。

pub mod arrange_selection;
pub mod automation;
pub mod batch_edit;
pub mod event;
pub mod font_scanner;
pub mod history;
pub mod midi_types;
pub mod note;
pub mod note_store;
pub mod pattern;
pub mod spatial_index;

pub use arrange_selection::ArrangeSelection;
pub use automation::{
    AutomationEdit, AutomationEvent, AutomationLane, AutomationTarget, SegmentShape,
};
pub use batch_edit::{BatchEditOperation, parse_batch_edit_input};
pub use event::{
    AutomationEvent as EventAutomationEvent, AutomationTarget as EventAutomationTarget, ChordEvent,
    KeySignatureEvent, LyricsEvent, MarkerEvent, ProgramChangeEvent, ScaleType,
    SegmentShape as EventSegmentShape, TimeSignatureEvent,
};
pub use font_scanner::{FontInfo, get_cached_fonts, prewarm_font_cache, scan_system_fonts};
pub use history::{
    EditorSnapshot, EventListDelta, EventListItem, EventListTarget, History, HistoryEntry, MoveOp,
    OpKind, OperationEntry, UndoAction,
};
pub use midi_types::{
    BendDisplay, BendPoint, CC_CONTROLLER_NAMES, CcData, CcDisplay, CcPoint, EditMode,
    PITCH_BEND_CENTER, TempoPoint, VelocityPoint,
};
pub use note::Note;
pub use note_store::{BitSet, NoteMut, NoteStore, NoteView};
pub use pattern::Pattern;
pub use spatial_index::{NoteRef, NoteSpatialIndex};
