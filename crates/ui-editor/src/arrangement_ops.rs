//! 工程走带视图音符操作（跨音轨）
//!
//! 提供 arrange_move_notes / arrange_erase / arrange_razor 等操作，
//! 直接修改 EditorData::track_notes，并在当前音轨受影响时同步 editor_data.notes。
//!
//! # 子模块
//! - `helpers`: 共享辅助函数与类型
//! - `move_notes`: 音符移动（arrange_move_notes）
//! - `erase`: 音符擦除（arrange_erase）
//! - `razor`: 音符切割（arrange_razor）
//! - `selection`: 选中查询与批量操作（arrangement_selected_notes / arrange_delete_selected_notes / arrange_apply_speed_change）
//! - `add_note`: 音符添加（arrange_add_note）
//! - `clipboard`: 剪贴板操作（复制/粘贴/剪切）

use super::Editor;

mod add_note;
mod clipboard;
mod erase;
mod helpers;
mod move_notes;
mod razor;
mod selection;
