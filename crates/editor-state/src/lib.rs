//! 编辑器状态管理模块
//!
//! 管理编辑器状态，包括画布状态、编辑数据、交互状态等。

pub mod editor_state;
pub mod editor_transform;

pub use editor_state::{
    CanvasState, DEFAULT_BPM, DEFAULT_PREVIEW_VELOCITY, DragState, EditState, EditorData,
    EditorState, GLUE_PROXIMITY_THRESHOLD, HitType, InteractionState, SELECTION_BOX_EDGE_THRESHOLD,
    SelectionHitType,
};
pub use editor_transform::EditorTransform;
