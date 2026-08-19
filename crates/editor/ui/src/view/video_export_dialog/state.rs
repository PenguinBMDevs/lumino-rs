//! 视频导出状态常量与类型重导出
//!
//! 从 `crate::state::root_state` 重导出类型，降低主文件耦合。

pub use crate::state::root_state::{
    MIDITRAIL_Z_FAR_MAX, VideoExportDialogState, VideoExportOverlayState,
};
